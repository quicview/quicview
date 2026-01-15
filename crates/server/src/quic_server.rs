//! QUIC Server for QuicView
//!
//! This module provides the main QUIC server that handles:
//! - Client connections with token-based authentication
//! - Screen streaming (captures and sends frames to clients)
//! - Input event reception (mouse/keyboard from clients)
//! - Clipboard synchronization
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      QuicServer                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │ QUIC        │  │ Screen      │  │ Input               │  │
//! │  │ Endpoint    │──│ Capturer    │  │ Injector            │  │
//! │  │ (quinn)     │  │ (scrap)     │  │ (enigo)             │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────┘  │
//! │         │                │                   ▲              │
//! │         │                ▼                   │              │
//! │         │         ScreenFrame          InputEvent           │
//! │         │                │                   │              │
//! │         └────────────────┴───────────────────┘              │
//! │                          │                                  │
//! │                    Client Connection                        │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Notify};
use tracing::{debug, info};

use transport::quic_data::{
    ClipboardMessage, InputEvent,
    ScreenEncoding, ScreenFrame, StreamType,
};

use quinn::{
    Connection, Endpoint, RecvStream, SendStream,
    ServerConfig as QuinnServerConfig,
};
use quinn::crypto::rustls as quinn_rustls;
use rustls::pki_types::CertificateDer;

/// ALPN protocol identifier
pub const ALPN_QUICVIEW: &[u8] = b"quicview/1";

/// Configuration for the QUIC server
#[derive(Debug, Clone)]
pub struct QuicServerConfig {
    /// Bind address for QUIC
    pub bind: SocketAddr,
    /// Authentication token (clients must provide this)
    pub auth_token: Option<String>,
    /// TLS certificate chain (DER-encoded)
    pub cert_chain: Vec<Vec<u8>>,
    /// TLS private key (DER-encoded)
    pub private_key: Vec<u8>,
    /// Maximum frame size for screen data (default 4MB)
    pub max_frame_size: usize,
    /// Screen capture FPS target
    pub target_fps: u32,
    /// JPEG quality (30-95)
    pub jpeg_quality: u8,
}

impl Default for QuicServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 21116)),
            auth_token: None,
            cert_chain: Vec::new(),
            private_key: Vec::new(),
            max_frame_size: 4 * 1024 * 1024,
            target_fps: 30,
            jpeg_quality: 75,
        }
    }
}

/// Events emitted by the QUIC server
#[derive(Debug)]
pub enum QuicServerEvent {
    /// A client connected
    ClientConnected { id: u64, addr: SocketAddr },
    /// A client disconnected
    ClientDisconnected { id: u64, reason: String },
    /// Input event received from a client
    InputReceived { client_id: u64, event: InputEvent },
    /// Clipboard received from a client
    ClipboardReceived { client_id: u64, msg: ClipboardMessage },
    /// Server error
    Error { message: String },
    /// Server started
    Started { addr: SocketAddr },
    /// Server stopped
    Stopped,
}

/// Connected client state
struct ClientState {
    _id: u64,
    _addr: SocketAddr,
    _connection: Connection,
    /// Channel to send screen frames to this client
    screen_tx: Option<mpsc::Sender<ScreenFrame>>,
}

/// Handle to a running QUIC server
pub struct QuicServerHandle {
    /// Local address
    pub addr: SocketAddr,
    /// Shutdown signal
    shutdown: Arc<Notify>,
    /// Join handle
    join: tokio::task::JoinHandle<()>,
    /// Channel to broadcast screen frames to all clients
    screen_broadcast_tx: mpsc::Sender<ScreenFrame>,
}

impl QuicServerHandle {
    /// Get the local address
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Send a screen frame to all connected clients
    pub async fn broadcast_screen(&self, frame: ScreenFrame) -> Result<()> {
        self.screen_broadcast_tx.send(frame).await
            .map_err(|_| anyhow::anyhow!("broadcast channel closed"))
    }

    /// Signal shutdown
    pub fn signal_shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    /// Graceful shutdown and wait
    pub async fn shutdown(self) {
        self.shutdown.notify_waiters();
        let _ = self.join.await;
    }
}

/// Start the QUIC server
pub async fn start_quic_server(
    config: QuicServerConfig,
) -> Result<(QuicServerHandle, mpsc::Receiver<QuicServerEvent>)> {
    // Build TLS config
    let (cert_chain, private_key) = if config.cert_chain.is_empty() {
        // Generate self-signed cert for dev
        let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        (vec![cert_der], key_der)
    } else {
        (config.cert_chain.clone(), config.private_key.clone())
    };

    let key = rustls::pki_types::PrivateKeyDer::try_from(private_key)
        .map_err(|_| anyhow::anyhow!("invalid private key"))?;
    let certs: Vec<CertificateDer> = cert_chain.into_iter().map(CertificateDer::from).collect();

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    server_crypto.alpn_protocols = vec![ALPN_QUICVIEW.to_vec()];

    let server_crypto = quinn_rustls::QuicServerConfig::try_from(server_crypto)?;
    let server_config = QuinnServerConfig::with_crypto(Arc::new(server_crypto));

    let endpoint = Endpoint::server(server_config, config.bind)?;
    let local_addr = endpoint.local_addr()?;

    let shutdown = Arc::new(Notify::new());
    let (event_tx, event_rx) = mpsc::channel::<QuicServerEvent>(64);
    let (screen_broadcast_tx, mut screen_broadcast_rx) = mpsc::channel::<ScreenFrame>(16);

    // Shared client state
    let clients: Arc<RwLock<HashMap<u64, ClientState>>> = Arc::new(RwLock::new(HashMap::new()));

    let shutdown_clone = shutdown.clone();
    let auth_token = config.auth_token.clone();
    let max_frame_size = config.max_frame_size;

    // Spawn screen broadcast task
    let clients_for_broadcast = clients.clone();
    tokio::spawn(async move {
        while let Some(frame) = screen_broadcast_rx.recv().await {
            let clients = clients_for_broadcast.read().await;
            for client in clients.values() {
                if let Some(tx) = &client.screen_tx {
                    let _ = tx.try_send(frame.clone());
                }
            }
        }
    });

    let event_tx_clone = event_tx.clone();
    let join = tokio::spawn(async move {
        let mut client_id_counter: u64 = 0;

        let _ = event_tx_clone.send(QuicServerEvent::Started { addr: local_addr }).await;

        loop {
            tokio::select! {
                Some(incoming) = endpoint.accept() => {
                    client_id_counter += 1;
                    let client_id = client_id_counter;
                    let remote_addr = incoming.remote_address();
                    let event_tx = event_tx_clone.clone();
                    let _auth_token = auth_token.clone();
                    let clients = clients.clone();

                    tokio::spawn(async move {
                        let conn = match incoming.await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = event_tx.send(QuicServerEvent::Error {
                                    message: format!("connection failed: {}", e),
                                }).await;
                                return;
                            }
                        };

                        // TODO: Implement auth handshake on first stream
                        // For now, accept all connections
                        
                        let _ = event_tx.send(QuicServerEvent::ClientConnected {
                            id: client_id,
                            addr: remote_addr,
                        }).await;

                        // Create screen channel for this client
                        let (screen_tx, mut screen_rx) = mpsc::channel::<ScreenFrame>(8);

                        // Store client state
                        {
                            let mut clients = clients.write().await;
                            clients.insert(client_id, ClientState {
                                _id: client_id,
                                _addr: remote_addr,
                                _connection: conn.clone(),
                                screen_tx: Some(screen_tx),
                            });
                        }

                        // Handle client streams
                        let conn_clone = conn.clone();
                        let event_tx_clone = event_tx.clone();
                        let clients_clone = clients.clone();

                        // Spawn stream handler
                        let stream_handler = tokio::spawn(async move {
                            loop {
                                match conn_clone.accept_bi().await {
                                    Ok((send, mut recv)) => {
                                        // Read stream type byte
                                        let mut type_buf = [0u8; 1];
                                        if recv.read_exact(&mut type_buf).await.is_err() {
                                            continue;
                                        }

                                        let stream_type = match StreamType::try_from(type_buf[0]) {
                                            Ok(t) => t,
                                            Err(_) => continue,
                                        };

                                        let event_tx = event_tx_clone.clone();
                                        tokio::spawn(handle_client_stream(
                                            client_id,
                                            stream_type,
                                            send,
                                            recv,
                                            max_frame_size,
                                            event_tx,
                                        ));
                                    }
                                    Err(_) => break,
                                }
                            }
                        });

                        // Spawn screen sender for this client
                        let screen_sender = tokio::spawn(async move {
                            // Open a unidirectional stream for screen frames
                            let mut send = match conn.open_uni().await {
                                Ok(s) => s,
                                Err(_) => return,
                            };

                            // Write stream type
                            if send.write_all(&[StreamType::Screen as u8]).await.is_err() {
                                return;
                            }

                            // Send frames as they arrive
                            while let Some(frame) = screen_rx.recv().await {
                                let payload = match serde_json::to_vec(&frame) {
                                    Ok(p) => p,
                                    Err(_) => continue,
                                };
                                let len = (payload.len() as u32).to_be_bytes();
                                if send.write_all(&len).await.is_err() {
                                    break;
                                }
                                if send.write_all(&payload).await.is_err() {
                                    break;
                                }
                            }
                        });

                        // Wait for client to disconnect
                        let _ = stream_handler.await;
                        screen_sender.abort();

                        // Remove client
                        {
                            let mut clients = clients_clone.write().await;
                            clients.remove(&client_id);
                        }

                        let _ = event_tx.send(QuicServerEvent::ClientDisconnected {
                            id: client_id,
                            reason: "connection closed".into(),
                        }).await;
                    });
                }
                _ = shutdown_clone.notified() => {
                    info!("QUIC server: shutdown signaled");
                    break;
                }
            }
        }

        endpoint.close(0u8.into(), b"shutdown");
        let _ = event_tx_clone.send(QuicServerEvent::Stopped).await;
    });

    Ok((
        QuicServerHandle {
            addr: local_addr,
            shutdown,
            join,
            screen_broadcast_tx,
        },
        event_rx,
    ))
}

/// Handle an incoming stream from a client
async fn handle_client_stream(
    client_id: u64,
    stream_type: StreamType,
    mut _send: SendStream,
    mut recv: RecvStream,
    max_frame_size: usize,
    event_tx: mpsc::Sender<QuicServerEvent>,
) {
    match stream_type {
        StreamType::Input => {
            // Read input events
            loop {
                match read_json_frame::<InputEvent>(&mut recv, max_frame_size).await {
                    Ok(event) => {
                        let _ = event_tx.send(QuicServerEvent::InputReceived {
                            client_id,
                            event,
                        }).await;
                    }
                    Err(_) => break,
                }
            }
        }
        StreamType::Clipboard => {
            // Read clipboard messages
            loop {
                match read_json_frame::<ClipboardMessage>(&mut recv, max_frame_size).await {
                    Ok(msg) => {
                        let _ = event_tx.send(QuicServerEvent::ClipboardReceived {
                            client_id,
                            msg,
                        }).await;
                    }
                    Err(_) => break,
                }
            }
        }
        _ => {
            // Screen streams are server→client, ignore from client
        }
    }
}

/// Helper to read a JSON frame
async fn read_json_frame<T: serde::de::DeserializeOwned>(
    recv: &mut RecvStream,
    max_size: usize,
) -> Result<T> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await.context("read length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_size {
        anyhow::bail!("frame too large");
    }
    let mut buf = vec![0u8; len];
    recv.read_exact(&mut buf).await.context("read data")?;
    let msg: T = serde_json::from_slice(&buf)?;
    Ok(msg)
}

// ============================================================================
// Screen Capture Integration
// ============================================================================

/// Screen capturer that produces frames for the QUIC server
pub struct ScreenCapturer {
    /// Target FPS
    pub fps: u32,
    /// JPEG quality
    pub quality: u8,
    /// Shutdown signal
    shutdown: Arc<Notify>,
}

impl ScreenCapturer {
    /// Create a new screen capturer
    pub fn new(fps: u32, quality: u8) -> Self {
        Self {
            fps,
            quality,
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Start capturing and sending frames to the server handle
    /// This is a placeholder - actual implementation would use scrap crate
    pub async fn run(&self, server: &QuicServerHandle) -> Result<()> {
        let frame_interval = std::time::Duration::from_millis(1000 / self.fps as u64);
        let mut seq: u64 = 0;
        let start = std::time::Instant::now();

        loop {
            tokio::select! {
                _ = self.shutdown.notified() => break,
                _ = tokio::time::sleep(frame_interval) => {
                    // Placeholder: generate a dummy frame
                    // In real implementation, this would capture the screen
                    let frame = ScreenFrame {
                        seq,
                        timestamp_ms: start.elapsed().as_millis() as u64,
                        width: 1920,
                        height: 1080,
                        encoding: ScreenEncoding::Jpeg,
                        data: vec![0xFF, 0xD8, 0xFF, 0xE0], // JPEG header stub
                    };
                    seq += 1;

                    if let Err(e) = server.broadcast_screen(frame).await {
                        debug!("broadcast error: {}", e);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    /// Stop capturing
    pub fn stop(&self) {
        self.shutdown.notify_waiters();
    }
}

// ============================================================================
// Input Injection Integration
// ============================================================================

/// Input injector that processes input events from clients
pub struct InputInjector {
    // In real implementation, this would hold enigo::Enigo
}

impl InputInjector {
    /// Create a new input injector
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Process an input event
    /// This is a placeholder - actual implementation would use enigo
    pub fn inject(&mut self, event: &InputEvent) -> Result<()> {
        match event {
            InputEvent::MouseMove { x, y } => {
                debug!("inject: mouse move to ({}, {})", x, y);
                // enigo.mouse_move_to(*x, *y);
            }
            InputEvent::MouseButton { button, pressed, .. } => {
                debug!("inject: mouse {:?} {}", button, if *pressed { "down" } else { "up" });
            }
            InputEvent::MouseScroll { delta_x, delta_y, .. } => {
                debug!("inject: scroll ({}, {})", delta_x, delta_y);
            }
            InputEvent::Key { code, pressed, .. } => {
                debug!("inject: key {} {}", code, if *pressed { "down" } else { "up" });
            }
            InputEvent::Text { text } => {
                debug!("inject: text \"{}\"", text);
            }
        }
        Ok(())
    }
}

impl Default for InputInjector {
    fn default() -> Self {
        Self::new().expect("input injector")
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quic_server_start_stop() {
        let config = QuicServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ..Default::default()
        };
        let (handle, mut events) = start_quic_server(config).await.expect("start");
        
        // Should receive Started event
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            events.recv(),
        ).await.expect("timeout").expect("event");
        
        match event {
            QuicServerEvent::Started { addr } => {
                assert!(addr.port() > 0);
            }
            _ => panic!("unexpected event"),
        }

        handle.shutdown().await;
    }
}
