//! QUIC Client for QuicView
//!
//! This module provides a high-level QUIC client for connecting to a QuicView server
//! and receiving screen frames while sending input events and clipboard messages.
//!
//! # Architecture
//!
//! The QUIC client wraps the transport layer's `DataClient` and provides:
//! - Connection management with automatic reconnection
//! - Screen frame reception with callback-based delivery
//! - Input event sending (mouse, keyboard)
//! - Clipboard synchronization
//!
//! # Example
//!
//! ```ignore
//! let config = QuicClientConfig {
//!     server_addr: "192.168.1.100:21116".parse()?,
//!     server_name: "quicview-server".into(),
//!     tls_mode: ClientTlsMode::Insecure, // dev only!
//!     ..Default::default()
//! };
//! let (client, events) = QuicViewClient::connect(config).await?;
//!
//! // Receive screen frames
//! while let Some(event) = events.recv().await {
//!     match event {
//!         QuicClientEvent::ScreenFrame(frame) => {
//!             // Render frame.data (JPEG/H264)
//!         }
//!         QuicClientEvent::ClipboardText(text) => {
//!             // Update local clipboard
//!         }
//!         _ => {}
//!     }
//! }
//! ```

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use transport::quic_data::{
    ClipboardMessage, DataClient, DataClientConfig,
    InputEvent, MouseButton, ScreenEncoding, ScreenFrame,
    TlsMode as TransportTlsMode,
};

/// Errors from the QUIC client
#[derive(Debug, Error)]
pub enum QuicClientError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    #[error("already connected")]
    AlreadyConnected,
    #[error("not connected")]
    NotConnected,
    #[error("send failed: {0}")]
    SendFailed(String),
    #[error("receive failed: {0}")]
    ReceiveFailed(String),
    #[error("transport error: {0}")]
    Transport(#[from] anyhow::Error),
}

/// TLS verification mode for the client
#[derive(Debug, Clone)]
pub enum ClientTlsMode {
    /// Verify against system root certificates
    System,
    /// Trust any certificate (development only!)
    Insecure,
    /// Pin a specific certificate by SHA256 hash (hex-encoded DER)
    Pin { sha256_hex: String },
}

impl Default for ClientTlsMode {
    fn default() -> Self {
        Self::Insecure // TODO: Change to System for production
    }
}

/// Configuration for the QUIC client
#[derive(Debug, Clone)]
pub struct QuicClientConfig {
    /// Server address to connect to
    pub server_addr: SocketAddr,
    /// Server hostname for TLS SNI verification
    pub server_name: String,
    /// TLS verification mode
    pub tls_mode: ClientTlsMode,
    /// Maximum frame size (default: 4MB)
    pub max_frame_size: usize,
    /// Reconnection enabled
    pub auto_reconnect: bool,
    /// Base reconnect delay in milliseconds
    pub reconnect_base_ms: u64,
    /// Maximum reconnect delay in milliseconds
    pub reconnect_max_ms: u64,
    /// Receive screen frames
    pub enable_screen: bool,
    /// Send input events
    pub enable_input: bool,
    /// Sync clipboard
    pub enable_clipboard: bool,
}

impl Default for QuicClientConfig {
    fn default() -> Self {
        Self {
            server_addr: SocketAddr::from(([127, 0, 0, 1], 21116)),
            server_name: "localhost".into(),
            tls_mode: ClientTlsMode::default(),
            max_frame_size: 4 * 1024 * 1024, // 4MB
            auto_reconnect: true,
            reconnect_base_ms: 500,
            reconnect_max_ms: 30_000,
            enable_screen: true,
            enable_input: true,
            enable_clipboard: true,
        }
    }
}

/// Events emitted by the QUIC client
#[derive(Debug, Clone)]
pub enum QuicClientEvent {
    /// Successfully connected to server
    Connected,
    /// Disconnected from server
    Disconnected { reason: String },
    /// Reconnecting after disconnect
    Reconnecting { attempt: u32 },
    /// Received a screen frame
    ScreenFrame(ScreenFrameEvent),
    /// Received clipboard content
    ClipboardText(String),
    /// Received clipboard binary
    ClipboardBinary { mime_type: String, data: Vec<u8> },
    /// Error occurred
    Error(String),
}

/// Screen frame event data
#[derive(Debug, Clone)]
pub struct ScreenFrameEvent {
    /// Frame sequence number
    pub seq: u64,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Encoding type
    pub encoding: ScreenEncodingType,
    /// Encoded frame data
    pub data: Vec<u8>,
}

/// Screen encoding type (mirrors transport layer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenEncodingType {
    Jpeg,
    H264,
    RawBgra,
}

impl From<ScreenEncoding> for ScreenEncodingType {
    fn from(e: ScreenEncoding) -> Self {
        match e {
            ScreenEncoding::Jpeg => Self::Jpeg,
            ScreenEncoding::H264 => Self::H264,
            ScreenEncoding::RawBgra => Self::RawBgra,
        }
    }
}

impl From<ScreenFrame> for ScreenFrameEvent {
    fn from(f: ScreenFrame) -> Self {
        Self {
            seq: f.seq,
            timestamp_ms: f.timestamp_ms,
            width: f.width,
            height: f.height,
            encoding: f.encoding.into(),
            data: f.data,
        }
    }
}

/// Handle to a running QUIC client connection
pub struct QuicViewClient {
    #[allow(dead_code)]
    config: QuicClientConfig,
    state: Arc<RwLock<ClientState>>,
    input_tx: Option<mpsc::Sender<InputEvent>>,
    clipboard_tx: Option<mpsc::Sender<ClipboardMessage>>,
    shutdown_tx: mpsc::Sender<()>,
    tasks: Vec<JoinHandle<()>>,
}

#[derive(Default)]
struct ClientState {
    connected: bool,
    reconnect_attempts: u32,
    last_frame_seq: u64,
    frames_received: u64,
    input_events_sent: u64,
}

impl QuicViewClient {
    /// Connect to a QuicView server
    pub async fn connect(
        config: QuicClientConfig,
    ) -> Result<(Self, mpsc::Receiver<QuicClientEvent>), QuicClientError> {
        let (event_tx, event_rx) = mpsc::channel(64);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        // Input and clipboard channels (if enabled)
        let (input_tx, input_rx) = if config.enable_input {
            let (tx, rx) = mpsc::channel(64);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let (clipboard_tx, clipboard_rx) = if config.enable_clipboard {
            let (tx, rx) = mpsc::channel(16);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        let state = Arc::new(RwLock::new(ClientState::default()));
        let state_clone = state.clone();
        let config_clone = config.clone();

        // Main connection task
        let event_tx_clone = event_tx.clone();
        let connection_task = tokio::spawn(async move {
            run_connection_loop(
                config_clone,
                state_clone,
                event_tx_clone,
                input_rx,
                clipboard_rx,
                shutdown_rx,
            )
            .await;
        });

        Ok((
            Self {
                config,
                state,
                input_tx,
                clipboard_tx,
                shutdown_tx,
                tasks: vec![connection_task],
            },
            event_rx,
        ))
    }

    /// Send a mouse move event
    pub async fn send_mouse_move(&self, x: i32, y: i32) -> Result<(), QuicClientError> {
        self.send_input(InputEvent::MouseMove { x, y }).await
    }

    /// Send a mouse button event
    pub async fn send_mouse_button(
        &self,
        button: MouseButtonType,
        pressed: bool,
        x: i32,
        y: i32,
    ) -> Result<(), QuicClientError> {
        self.send_input(InputEvent::MouseButton {
            button: button.into(),
            pressed,
            x,
            y,
        })
        .await
    }

    /// Send a mouse scroll event
    pub async fn send_mouse_scroll(
        &self,
        delta_x: i32,
        delta_y: i32,
        x: i32,
        y: i32,
    ) -> Result<(), QuicClientError> {
        self.send_input(InputEvent::MouseScroll {
            delta_x,
            delta_y,
            x,
            y,
        })
        .await
    }

    /// Send a key event
    pub async fn send_key(
        &self,
        code: u32,
        pressed: bool,
        modifiers: u8,
    ) -> Result<(), QuicClientError> {
        self.send_input(InputEvent::Key {
            code,
            pressed,
            modifiers,
        })
        .await
    }

    /// Send text input (for IME)
    pub async fn send_text(&self, text: String) -> Result<(), QuicClientError> {
        self.send_input(InputEvent::Text { text }).await
    }

    /// Send clipboard text to server
    pub async fn send_clipboard_text(&self, text: String) -> Result<(), QuicClientError> {
        self.send_clipboard(ClipboardMessage::Text(text)).await
    }

    /// Request clipboard content from server
    pub async fn request_clipboard(&self) -> Result<(), QuicClientError> {
        self.send_clipboard(ClipboardMessage::Request).await
    }

    /// Check if currently connected
    pub async fn is_connected(&self) -> bool {
        self.state.read().await.connected
    }

    /// Get connection statistics
    pub async fn stats(&self) -> ClientStats {
        let s = self.state.read().await;
        ClientStats {
            connected: s.connected,
            frames_received: s.frames_received,
            input_events_sent: s.input_events_sent,
            last_frame_seq: s.last_frame_seq,
        }
    }

    /// Disconnect and shutdown the client
    pub async fn disconnect(mut self) {
        let _ = self.shutdown_tx.send(()).await;
        for task in self.tasks.drain(..) {
            task.abort();
            let _ = task.await;
        }
    }

    async fn send_input(&self, event: InputEvent) -> Result<(), QuicClientError> {
        let tx = self.input_tx.as_ref().ok_or(QuicClientError::NotConnected)?;
        tx.send(event)
            .await
            .map_err(|e| QuicClientError::SendFailed(e.to_string()))?;
        self.state.write().await.input_events_sent += 1;
        Ok(())
    }

    async fn send_clipboard(&self, msg: ClipboardMessage) -> Result<(), QuicClientError> {
        let tx = self
            .clipboard_tx
            .as_ref()
            .ok_or(QuicClientError::NotConnected)?;
        tx.send(msg)
            .await
            .map_err(|e| QuicClientError::SendFailed(e.to_string()))?;
        Ok(())
    }
}

/// Mouse button type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButtonType {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl From<MouseButtonType> for MouseButton {
    fn from(b: MouseButtonType) -> Self {
        match b {
            MouseButtonType::Left => MouseButton::Left,
            MouseButtonType::Right => MouseButton::Right,
            MouseButtonType::Middle => MouseButton::Middle,
            MouseButtonType::Back => MouseButton::Back,
            MouseButtonType::Forward => MouseButton::Forward,
        }
    }
}

/// Client statistics
#[derive(Debug, Clone, Default)]
pub struct ClientStats {
    pub connected: bool,
    pub frames_received: u64,
    pub input_events_sent: u64,
    pub last_frame_seq: u64,
}

/// Convert client TLS mode to transport TLS mode
fn to_transport_tls_mode(mode: &ClientTlsMode) -> TransportTlsMode {
    match mode {
        ClientTlsMode::System => TransportTlsMode::System,
        ClientTlsMode::Insecure => TransportTlsMode::Insecure,
        ClientTlsMode::Pin { sha256_hex } => TransportTlsMode::Pin {
            sha256_hex: sha256_hex.clone(),
        },
    }
}

/// Main connection loop with reconnection support
async fn run_connection_loop(
    config: QuicClientConfig,
    state: Arc<RwLock<ClientState>>,
    event_tx: mpsc::Sender<QuicClientEvent>,
    mut input_rx: Option<mpsc::Receiver<InputEvent>>,
    mut clipboard_rx: Option<mpsc::Receiver<ClipboardMessage>>,
    mut shutdown_rx: mpsc::Receiver<()>,
) {
    let mut attempt: u32 = 0;

    loop {
        // Check for shutdown
        if shutdown_rx.try_recv().is_ok() {
            break;
        }

        // Attempt connection
        let transport_config = DataClientConfig {
            server_addr: config.server_addr,
            server_name: config.server_name.clone(),
            tls_mode: to_transport_tls_mode(&config.tls_mode),
            max_frame_size: config.max_frame_size,
        };

        match DataClient::connect(transport_config).await {
            Ok(client) => {
                attempt = 0;
                {
                    let mut s = state.write().await;
                    s.connected = true;
                    s.reconnect_attempts = 0;
                }
                let _ = event_tx.send(QuicClientEvent::Connected).await;

                // Run the session until disconnect
                let disconnect_reason = run_session(
                    &client,
                    &config,
                    &state,
                    &event_tx,
                    &mut input_rx,
                    &mut clipboard_rx,
                    &mut shutdown_rx,
                )
                .await;

                // Mark disconnected
                {
                    let mut s = state.write().await;
                    s.connected = false;
                }
                let _ = event_tx
                    .send(QuicClientEvent::Disconnected {
                        reason: disconnect_reason.clone(),
                    })
                    .await;

                // Check if we should stop
                if disconnect_reason == "shutdown" || !config.auto_reconnect {
                    break;
                }
            }
            Err(e) => {
                attempt = attempt.saturating_add(1);
                let _ = event_tx
                    .send(QuicClientEvent::Error(format!(
                        "connection failed: {}",
                        e
                    )))
                    .await;

                if !config.auto_reconnect {
                    break;
                }
            }
        }

        // Reconnect backoff
        if config.auto_reconnect {
            attempt = attempt.saturating_add(1);
            let _ = event_tx
                .send(QuicClientEvent::Reconnecting { attempt })
                .await;

            let delay = calculate_backoff(attempt, config.reconnect_base_ms, config.reconnect_max_ms);
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
    }
}

/// Run a connected session
async fn run_session(
    client: &DataClient,
    config: &QuicClientConfig,
    state: &Arc<RwLock<ClientState>>,
    event_tx: &mpsc::Sender<QuicClientEvent>,
    input_rx: &mut Option<mpsc::Receiver<InputEvent>>,
    clipboard_rx: &mut Option<mpsc::Receiver<ClipboardMessage>>,
    shutdown_rx: &mut mpsc::Receiver<()>,
) -> String {
    // Open streams based on config
    let screen_result = if config.enable_screen {
        client.open_screen_stream().await.ok()
    } else {
        None
    };

    let input_result = if config.enable_input {
        client.open_input_stream().await.ok()
    } else {
        None
    };

    let clipboard_result = if config.enable_clipboard {
        client.open_clipboard_stream().await.ok()
    } else {
        None
    };

    let mut screen_recv = screen_result;
    let mut input_send = input_result;
    let (mut clipboard_send, mut clipboard_recv) = match clipboard_result {
        Some((s, r)) => (Some(s), Some(r)),
        None => (None, None),
    };

    loop {
        tokio::select! {
            // Shutdown signal
            _ = shutdown_rx.recv() => {
                client.close("client shutdown");
                return "shutdown".to_string();
            }

            // Receive screen frames
            frame = async {
                if let Some(ref mut recv) = screen_recv {
                    recv.recv().await
                } else {
                    // Never resolves if screen is disabled
                    std::future::pending().await
                }
            } => {
                match frame {
                    Ok(f) => {
                        let mut s = state.write().await;
                        s.frames_received += 1;
                        s.last_frame_seq = f.seq;
                        drop(s);
                        let _ = event_tx.send(QuicClientEvent::ScreenFrame(f.into())).await;
                    }
                    Err(e) => {
                        return format!("screen recv error: {}", e);
                    }
                }
            }

            // Send input events
            input = async {
                if let Some(ref mut rx) = input_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(event) = input {
                    if let Some(ref mut send) = input_send {
                        if let Err(e) = send.send(&event).await {
                            let _ = event_tx.send(QuicClientEvent::Error(format!("input send: {}", e))).await;
                        }
                    }
                }
            }

            // Send clipboard messages
            clip_out = async {
                if let Some(ref mut rx) = clipboard_rx {
                    rx.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                if let Some(msg) = clip_out {
                    if let Some(ref mut send) = clipboard_send {
                        if let Err(e) = send.send(&msg).await {
                            let _ = event_tx.send(QuicClientEvent::Error(format!("clipboard send: {}", e))).await;
                        }
                    }
                }
            }

            // Receive clipboard messages
            clip_in = async {
                if let Some(ref mut recv) = clipboard_recv {
                    recv.recv().await
                } else {
                    std::future::pending().await
                }
            } => {
                match clip_in {
                    Ok(ClipboardMessage::Text(text)) => {
                        let _ = event_tx.send(QuicClientEvent::ClipboardText(text)).await;
                    }
                    Ok(ClipboardMessage::Binary { mime_type, data }) => {
                        let _ = event_tx.send(QuicClientEvent::ClipboardBinary { mime_type, data }).await;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        return format!("clipboard recv error: {}", e);
                    }
                }
            }
        }
    }
}

/// Calculate backoff delay with jitter
fn calculate_backoff(attempt: u32, base_ms: u64, max_ms: u64) -> u64 {
    let base = base_ms.max(1);
    let cap = max_ms.max(base);
    let pow = 1u64 << attempt.min(20);
    let delay = base.saturating_mul(pow).min(cap);
    // Add jitter (up to 25% of delay)
    let jitter = (delay / 4).min(1000);
    let jitter_val = if jitter > 0 {
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64)
            % jitter
    } else {
        0
    };
    delay + jitter_val
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use transport::quic_data::{start_data_server, DataServerConfig, DataServerEvent};

    #[tokio::test]
    async fn test_quic_client_connect_and_receive_input() {
        // Start a test server
        let server_config = DataServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ..Default::default()
        };
        let (server_handle, mut server_events) =
            start_data_server(server_config).await.expect("start server");
        let server_addr = server_handle.local_addr();

        // Connect client
        let client_config = QuicClientConfig {
            server_addr,
            server_name: "localhost".into(),
            tls_mode: ClientTlsMode::Insecure,
            enable_screen: false, // Disable screen for this test
            ..Default::default()
        };
        let (client, mut events) = QuicViewClient::connect(client_config)
            .await
            .expect("connect");

        // Wait for connected event
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("timeout")
            .expect("event");
        assert!(matches!(event, QuicClientEvent::Connected));

        // Send a mouse move
        client.send_mouse_move(100, 200).await.expect("send");

        // Server should receive it
        let server_event = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(e) = server_events.recv().await {
                    match e {
                        DataServerEvent::InputReceived { event, .. } => return event,
                        _ => continue,
                    }
                }
            }
        })
        .await
        .expect("server event");

        match server_event {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, 100);
                assert_eq!(y, 200);
            }
            _ => panic!("unexpected event"),
        }

        // Cleanup
        client.disconnect().await;
        server_handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_client_stats() {
        let config = QuicClientConfig::default();
        // We can't actually connect without a server, but we can test the config
        assert_eq!(config.server_addr, SocketAddr::from(([127, 0, 0, 1], 21116)));
        assert!(config.auto_reconnect);
        assert!(config.enable_screen);
        assert!(config.enable_input);
        assert!(config.enable_clipboard);
    }

    #[test]
    fn test_backoff_calculation() {
        let delay0 = calculate_backoff(0, 500, 30000);
        assert!(delay0 >= 500 && delay0 <= 625); // 500 + up to 25% jitter

        let delay1 = calculate_backoff(1, 500, 30000);
        assert!(delay1 >= 1000 && delay1 <= 1250);

        let delay_max = calculate_backoff(10, 500, 30000);
        assert!(delay_max >= 30000 && delay_max <= 37500);
    }
}
