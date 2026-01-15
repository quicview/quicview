//! QUIC Data Streams for QuicView
//!
//! This module provides multiplexed QUIC streams for real-time data transfer:
//! - **Screen frames**: Server → Client (unidirectional)
//! - **Input events**: Client → Server (unidirectional)
//! - **Clipboard**: Bidirectional sync
//! - **File transfer**: Future extension
//!
//! # Stream Multiplexing Design
//!
//! ```text
//! QUIC Connection
//! ├── Stream 0 (bidirectional): Control channel (see quic_ctrl)
//! ├── Stream 1 (server→client): Screen frames (JPEG/H264)
//! ├── Stream 2 (client→server): Input events (mouse/keyboard)
//! ├── Stream 3 (bidirectional): Clipboard sync
//! └── Stream 4+ (future): File transfer, audio, etc.
//! ```
//!
//! # Framing
//!
//! All streams use a simple length-prefixed framing:
//! ```text
//! ┌─────────────┬──────────────────────────────┐
//! │ Length (4B) │ Payload (protobuf or JPEG)   │
//! └─────────────┴──────────────────────────────┘
//! ```

use anyhow::{Context, Result};
use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;

#[cfg(feature = "quic")]
use quinn::{
    Connection, Endpoint, RecvStream, SendStream,
    ServerConfig as QuinnServerConfig, ClientConfig as QuinnClientConfig,
};
#[cfg(feature = "quic")]
use quinn::crypto::rustls as quinn_rustls;
#[cfg(feature = "quic")]
use rustls::pki_types::CertificateDer;
#[cfg(feature = "quic")]
use tokio::sync::mpsc;

/// ALPN protocol identifier for QuicView data streams
pub const ALPN_DATA: &[u8] = b"quicview/data/1";

/// Stream type identifiers (sent as first byte on stream open)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StreamType {
    /// Screen frames: server → client
    Screen = 0x01,
    /// Input events: client → server
    Input = 0x02,
    /// Clipboard sync: bidirectional
    Clipboard = 0x03,
    /// File transfer: bidirectional (future)
    FileTransfer = 0x04,
}

impl TryFrom<u8> for StreamType {
    type Error = anyhow::Error;
    fn try_from(v: u8) -> Result<Self> {
        match v {
            0x01 => Ok(Self::Screen),
            0x02 => Ok(Self::Input),
            0x03 => Ok(Self::Clipboard),
            0x04 => Ok(Self::FileTransfer),
            _ => anyhow::bail!("unknown stream type: {:#x}", v),
        }
    }
}

// ============================================================================
// Frame Types
// ============================================================================

/// Screen frame sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenFrame {
    /// Frame sequence number
    pub seq: u64,
    /// Timestamp in milliseconds since stream start
    pub timestamp_ms: u64,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Encoding format
    pub encoding: ScreenEncoding,
    /// Encoded frame data (JPEG, H264 NAL unit, etc.)
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Screen encoding formats
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScreenEncoding {
    /// JPEG image (simple, good for low FPS)
    Jpeg,
    /// H.264 NAL unit (efficient for video)
    H264,
    /// Raw BGRA pixels (debugging only)
    RawBgra,
}

/// Input event sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    /// Mouse move (absolute coordinates)
    MouseMove { x: i32, y: i32 },
    /// Mouse button press/release
    MouseButton { button: MouseButton, pressed: bool, x: i32, y: i32 },
    /// Mouse scroll
    MouseScroll { delta_x: i32, delta_y: i32, x: i32, y: i32 },
    /// Key press/release
    Key { code: u32, pressed: bool, modifiers: u8 },
    /// Text input (for IME)
    Text { text: String },
}

/// Mouse button identifiers
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

/// Clipboard message (bidirectional)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardMessage {
    /// Request current clipboard content
    Request,
    /// Clipboard content (text for now)
    Text(String),
    /// Clipboard content (binary/file, future)
    Binary { mime_type: String, data: Vec<u8> },
    /// Clear clipboard
    Clear,
}

// ============================================================================
// Framing Utilities
// ============================================================================

/// Write a length-prefixed frame to a QUIC send stream
#[cfg(feature = "quic")]
pub async fn write_frame(send: &mut SendStream, data: &[u8]) -> Result<()> {
    let len = data.len() as u32;
    send.write_all(&len.to_be_bytes()).await?;
    send.write_all(data).await?;
    Ok(())
}

/// Read a length-prefixed frame from a QUIC recv stream
#[cfg(feature = "quic")]
pub async fn read_frame(recv: &mut RecvStream, max_size: usize) -> Result<Bytes> {
    let mut len_buf = [0u8; 4];
    recv.read_exact(&mut len_buf).await.context("read frame length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_size {
        anyhow::bail!("frame too large: {} > {}", len, max_size);
    }
    let mut buf = BytesMut::zeroed(len);
    recv.read_exact(&mut buf).await.context("read frame data")?;
    Ok(buf.freeze())
}

/// Write a JSON-serializable message as a frame
#[cfg(feature = "quic")]
pub async fn write_json_frame<T: Serialize>(send: &mut SendStream, msg: &T) -> Result<()> {
    let payload = serde_json::to_vec(msg)?;
    write_frame(send, &payload).await
}

/// Read a JSON-deserializable message from a frame
#[cfg(feature = "quic")]
pub async fn read_json_frame<T: for<'de> Deserialize<'de>>(recv: &mut RecvStream, max_size: usize) -> Result<T> {
    let data = read_frame(recv, max_size).await?;
    let msg: T = serde_json::from_slice(&data)?;
    Ok(msg)
}

// ============================================================================
// Server-Side Data Endpoint
// ============================================================================

/// Configuration for the QUIC data server
#[derive(Debug, Clone)]
pub struct DataServerConfig {
    /// Bind address
    pub bind: SocketAddr,
    /// TLS certificate chain (DER-encoded)
    pub cert_chain: Vec<Vec<u8>>,
    /// TLS private key (DER-encoded)
    pub private_key: Vec<u8>,
    /// Maximum frame size (default: 4MB for screen frames)
    pub max_frame_size: usize,
}

impl Default for DataServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 21116)),
            cert_chain: Vec::new(),
            private_key: Vec::new(),
            max_frame_size: 4 * 1024 * 1024, // 4MB
        }
    }
}

/// Handle to a running data server
#[cfg(feature = "quic")]
pub struct DataServerHandle {
    /// Local address the server is bound to
    pub addr: SocketAddr,
    /// Endpoint for sending to connected clients
    endpoint: Endpoint,
    /// Shutdown signal
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    /// Join handle for the server task
    join: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "quic")]
impl DataServerHandle {
    /// Get the local address
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Graceful shutdown
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        self.endpoint.close(0u8.into(), b"shutdown");
        let _ = self.join.await;
    }
}

/// Events emitted by the data server
#[derive(Debug)]
pub enum DataServerEvent {
    /// New client connected
    ClientConnected { id: u64, addr: SocketAddr },
    /// Client disconnected
    ClientDisconnected { id: u64, reason: String },
    /// Input event received from client
    InputReceived { client_id: u64, event: InputEvent },
    /// Clipboard message received from client
    ClipboardReceived { client_id: u64, msg: ClipboardMessage },
    /// Error
    Error { client_id: Option<u64>, error: String },
}

/// Start a QUIC data server
///
/// Returns a handle and a receiver for server events.
#[cfg(feature = "quic")]
pub async fn start_data_server(
    config: DataServerConfig,
) -> Result<(DataServerHandle, mpsc::Receiver<DataServerEvent>)> {
    // Build TLS config
    let (cert_chain, private_key) = if config.cert_chain.is_empty() {
        // Generate self-signed cert for dev
        let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        (vec![cert_der], key_der)
    } else {
        (config.cert_chain, config.private_key)
    };

    let key = rustls::pki_types::PrivateKeyDer::try_from(private_key)
        .map_err(|_| anyhow::anyhow!("invalid private key"))?;
    let certs: Vec<CertificateDer> = cert_chain.into_iter().map(CertificateDer::from).collect();

    let mut server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    server_crypto.alpn_protocols = vec![ALPN_DATA.to_vec()];

    let server_crypto = quinn_rustls::QuicServerConfig::try_from(server_crypto)?;
    let server_config = QuinnServerConfig::with_crypto(Arc::new(server_crypto));

    let endpoint = Endpoint::server(server_config, config.bind)?;
    let local_addr = endpoint.local_addr()?;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let (event_tx, event_rx) = mpsc::channel::<DataServerEvent>(64);

    let endpoint_clone = endpoint.clone();
    let max_frame_size = config.max_frame_size;

    let join = tokio::spawn(async move {
        let mut client_id_counter: u64 = 0;

        loop {
            tokio::select! {
                Some(incoming) = endpoint_clone.accept() => {
                    client_id_counter += 1;
                    let client_id = client_id_counter;
                    let event_tx = event_tx.clone();
                    let remote_addr = incoming.remote_address();

                    tokio::spawn(async move {
                        let conn = match incoming.await {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = event_tx.send(DataServerEvent::Error {
                                    client_id: Some(client_id),
                                    error: format!("connection failed: {}", e),
                                }).await;
                                return;
                            }
                        };

                        let _ = event_tx.send(DataServerEvent::ClientConnected {
                            id: client_id,
                            addr: remote_addr,
                        }).await;

                        // Handle incoming streams from this client
                        loop {
                            match conn.accept_bi().await {
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

                                    let event_tx = event_tx.clone();
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

                        let _ = event_tx.send(DataServerEvent::ClientDisconnected {
                            id: client_id,
                            reason: "connection closed".into(),
                        }).await;
                    });
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    Ok((
        DataServerHandle {
            addr: local_addr,
            endpoint,
            shutdown_tx,
            join,
        },
        event_rx,
    ))
}

/// Handle an incoming stream from a client
#[cfg(feature = "quic")]
async fn handle_client_stream(
    client_id: u64,
    stream_type: StreamType,
    mut _send: SendStream,
    mut recv: RecvStream,
    max_frame_size: usize,
    event_tx: mpsc::Sender<DataServerEvent>,
) {
    match stream_type {
        StreamType::Input => {
            // Read input events
            loop {
                match read_json_frame::<InputEvent>(&mut recv, max_frame_size).await {
                    Ok(event) => {
                        let _ = event_tx.send(DataServerEvent::InputReceived {
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
                        let _ = event_tx.send(DataServerEvent::ClipboardReceived {
                            client_id,
                            msg,
                        }).await;
                    }
                    Err(_) => break,
                }
            }
        }
        _ => {
            // Unsupported stream type from client
        }
    }
}

// ============================================================================
// Client-Side Data Connection
// ============================================================================

/// Configuration for QUIC data client
#[derive(Debug, Clone)]
pub struct DataClientConfig {
    /// Server address
    pub server_addr: SocketAddr,
    /// Server hostname for TLS SNI
    pub server_name: String,
    /// TLS verification mode
    pub tls_mode: TlsMode,
    /// Maximum frame size
    pub max_frame_size: usize,
}

/// TLS verification mode for client
#[derive(Debug, Clone)]
pub enum TlsMode {
    /// Verify against system roots
    System,
    /// Trust any certificate (dev only!)
    Insecure,
    /// Pin a specific certificate SHA256 hash
    Pin { sha256_hex: String },
}

impl Default for DataClientConfig {
    fn default() -> Self {
        Self {
            server_addr: SocketAddr::from(([127, 0, 0, 1], 21116)),
            server_name: "localhost".into(),
            tls_mode: TlsMode::Insecure,
            max_frame_size: 4 * 1024 * 1024,
        }
    }
}

/// Handle to a data client connection
#[cfg(feature = "quic")]
pub struct DataClient {
    connection: Connection,
    config: DataClientConfig,
}

#[cfg(feature = "quic")]
impl DataClient {
    /// Connect to a QuicView server
    pub async fn connect(config: DataClientConfig) -> Result<Self> {
        let client_config = build_data_client_config(&config.tls_mode)?;

        let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        endpoint.set_default_client_config(client_config);

        let connection = endpoint
            .connect(config.server_addr, &config.server_name)?
            .await
            .context("QUIC handshake failed")?;

        Ok(Self { connection, config })
    }

    /// Open a screen stream to receive frames
    pub async fn open_screen_stream(&self) -> Result<ScreenReceiver> {
        let (mut send, recv) = self.connection.open_bi().await?;
        send.write_all(&[StreamType::Screen as u8]).await?;
        Ok(ScreenReceiver {
            recv,
            max_frame_size: self.config.max_frame_size,
        })
    }

    /// Open an input stream to send events
    pub async fn open_input_stream(&self) -> Result<InputSender> {
        let (mut send, _recv) = self.connection.open_bi().await?;
        send.write_all(&[StreamType::Input as u8]).await?;
        Ok(InputSender { send })
    }

    /// Open a clipboard stream for bidirectional sync
    pub async fn open_clipboard_stream(&self) -> Result<(ClipboardSender, ClipboardReceiver)> {
        let (mut send, recv) = self.connection.open_bi().await?;
        send.write_all(&[StreamType::Clipboard as u8]).await?;
        Ok((
            ClipboardSender { send },
            ClipboardReceiver {
                recv,
                max_frame_size: self.config.max_frame_size,
            },
        ))
    }

    /// Close the connection
    pub fn close(&self, reason: &str) {
        self.connection.close(0u8.into(), reason.as_bytes());
    }
}

/// Receiver for screen frames
#[cfg(feature = "quic")]
pub struct ScreenReceiver {
    recv: RecvStream,
    max_frame_size: usize,
}

#[cfg(feature = "quic")]
impl ScreenReceiver {
    /// Receive the next screen frame
    pub async fn recv(&mut self) -> Result<ScreenFrame> {
        read_json_frame(&mut self.recv, self.max_frame_size).await
    }
}

/// Sender for input events
#[cfg(feature = "quic")]
pub struct InputSender {
    send: SendStream,
}

#[cfg(feature = "quic")]
impl InputSender {
    /// Send an input event
    pub async fn send(&mut self, event: &InputEvent) -> Result<()> {
        write_json_frame(&mut self.send, event).await
    }
}

/// Sender for clipboard messages
#[cfg(feature = "quic")]
pub struct ClipboardSender {
    send: SendStream,
}

#[cfg(feature = "quic")]
impl ClipboardSender {
    /// Send a clipboard message
    pub async fn send(&mut self, msg: &ClipboardMessage) -> Result<()> {
        write_json_frame(&mut self.send, msg).await
    }
}

/// Receiver for clipboard messages
#[cfg(feature = "quic")]
pub struct ClipboardReceiver {
    recv: RecvStream,
    max_frame_size: usize,
}

#[cfg(feature = "quic")]
impl ClipboardReceiver {
    /// Receive a clipboard message
    pub async fn recv(&mut self) -> Result<ClipboardMessage> {
        read_json_frame(&mut self.recv, self.max_frame_size).await
    }
}

// ============================================================================
// TLS Configuration Helpers
// ============================================================================

#[cfg(feature = "quic")]
fn build_data_client_config(mode: &TlsMode) -> Result<QuinnClientConfig> {
    use rustls::{
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{ServerName, UnixTime},
        DigitallySignedStruct, SignatureScheme,
    };
    use sha2::{Digest, Sha256};
    use hex::ToHex;

    #[derive(Debug)]
    struct InsecureVerifier;
    impl ServerCertVerifier for InsecureVerifier {
        fn verify_server_cert(
            &self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>,
            _: &[u8], _: UnixTime,
        ) -> std::result::Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256]
        }
    }

    #[derive(Debug)]
    struct PinVerifier { sha256_hex: String }
    impl ServerCertVerifier for PinVerifier {
        fn verify_server_cert(
            &self, end_entity: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>,
            _: &[u8], _: UnixTime,
        ) -> std::result::Result<ServerCertVerified, rustls::Error> {
            let mut hasher = Sha256::new();
            hasher.update(end_entity.as_ref());
            let got: String = hasher.finalize().encode_hex();
            if got.eq_ignore_ascii_case(&self.sha256_hex) {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("certificate pin mismatch".into()))
            }
        }
        fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256]
        }
    }

    let mut roots = rustls::RootCertStore::empty();
    if matches!(mode, TlsMode::System) {
        for cert in rustls_native_certs::load_native_certs()? {
            let _ = roots.add(cert);
        }
    }

    let mut client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    client_crypto.alpn_protocols = vec![ALPN_DATA.to_vec()];

    match mode {
        TlsMode::System => {}
        TlsMode::Insecure => {
            client_crypto.dangerous().set_certificate_verifier(Arc::new(InsecureVerifier));
        }
        TlsMode::Pin { sha256_hex } => {
            client_crypto.dangerous().set_certificate_verifier(Arc::new(PinVerifier {
                sha256_hex: sha256_hex.clone(),
            }));
        }
    }

    let client_crypto = quinn_rustls::QuicClientConfig::try_from(client_crypto)?;
    Ok(QuinnClientConfig::new(Arc::new(client_crypto)))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "quic"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_data_server_start_stop() {
        let config = DataServerConfig::default();
        let (handle, _rx) = start_data_server(config).await.expect("start server");
        assert!(handle.addr.port() > 0);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_client_connect_and_input() {
        // Start server
        let server_config = DataServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ..Default::default()
        };
        let (server_handle, mut events) = start_data_server(server_config).await.expect("start server");
        let server_addr = server_handle.addr;

        // Connect client
        let client_config = DataClientConfig {
            server_addr,
            server_name: "localhost".into(),
            tls_mode: TlsMode::Insecure,
            ..Default::default()
        };
        let client = DataClient::connect(client_config).await.expect("connect");

        // Open input stream and send an event
        let mut input = client.open_input_stream().await.expect("open input");
        input.send(&InputEvent::MouseMove { x: 100, y: 200 }).await.expect("send");

        // Server should receive the event (with timeout)
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            async {
                loop {
                    if let Some(e) = events.recv().await {
                        match e {
                            DataServerEvent::InputReceived { event, .. } => return event,
                            _ => continue,
                        }
                    }
                }
            },
        ).await.expect("timeout");

        match event {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("unexpected event"),
        }

        client.close("test done");
        server_handle.shutdown().await;
    }
}
