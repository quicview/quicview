//! TCP+TLS Data Streams for QuicView
//!
//! This module provides a TCP+TLS fallback transport for environments where
//! UDP/QUIC is blocked. It uses the same framing and message types as the
//! QUIC transport but over a single TCP connection with TLS encryption.
//!
//! # Connection Model
//!
//! Unlike QUIC's native stream multiplexing, TCP+TLS uses a single connection
//! with an application-layer multiplexing scheme:
//!
//! ```text
//! TCP Connection (TLS 1.3)
//! └── Multiplexed frames with channel ID prefix
//!     ┌──────────┬─────────────┬──────────────────────────┐
//!     │ Chan (1B)│ Length (4B) │ Payload                  │
//!     └──────────┴─────────────┴──────────────────────────┘
//! ```
//!
//! Channel IDs:
//! - 0x01: Screen frames (server → client)
//! - 0x02: Input events (client → server)
//! - 0x03: Clipboard (bidirectional)

use anyhow::{Context, Result};
use bytes::{Bytes, BytesMut};
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[cfg(feature = "quic")]
use rustls::pki_types::CertificateDer;
#[cfg(feature = "quic")]
use tokio_rustls::{TlsAcceptor, TlsConnector, client::TlsStream as ClientTlsStream, server::TlsStream as ServerTlsStream};

// Re-export frame types from quic_data
pub use crate::quic_data::{
    ClipboardMessage, InputEvent, MouseButton, ScreenEncoding, ScreenFrame, StreamType,
};

/// Channel IDs for multiplexing over TCP
pub mod channel {
    pub const SCREEN: u8 = 0x01;
    pub const INPUT: u8 = 0x02;
    pub const CLIPBOARD: u8 = 0x03;
}

// ============================================================================
// Framing Utilities (TCP version with channel prefix)
// ============================================================================

/// Write a multiplexed frame to a TCP stream
pub async fn write_tcp_frame<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    channel: u8,
    data: &[u8],
) -> Result<()> {
    let len = data.len() as u32;
    writer.write_u8(channel).await?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a multiplexed frame from a TCP stream
/// Returns (channel_id, data)
pub async fn read_tcp_frame<R: AsyncReadExt + Unpin>(
    reader: &mut R,
    max_size: usize,
) -> Result<(u8, Bytes)> {
    let channel = reader.read_u8().await.context("read channel")?;
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await.context("read length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > max_size {
        anyhow::bail!("frame too large: {} > {}", len, max_size);
    }
    let mut buf = BytesMut::zeroed(len);
    reader.read_exact(&mut buf).await.context("read data")?;
    Ok((channel, buf.freeze()))
}

/// Write a JSON message to a specific channel
pub async fn write_tcp_json<W: AsyncWriteExt + Unpin, T: Serialize>(
    writer: &mut W,
    channel: u8,
    msg: &T,
) -> Result<()> {
    let payload = serde_json::to_vec(msg)?;
    write_tcp_frame(writer, channel, &payload).await
}

// ============================================================================
// Server-Side TCP+TLS
// ============================================================================

/// Configuration for TCP+TLS data server
#[derive(Debug, Clone)]
pub struct TcpServerConfig {
    /// Bind address
    pub bind: SocketAddr,
    /// TLS certificate chain (DER-encoded)
    pub cert_chain: Vec<Vec<u8>>,
    /// TLS private key (DER-encoded)
    pub private_key: Vec<u8>,
    /// Maximum frame size
    pub max_frame_size: usize,
}

impl Default for TcpServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 21116)),
            cert_chain: Vec::new(),
            private_key: Vec::new(),
            max_frame_size: 4 * 1024 * 1024,
        }
    }
}

/// Handle to a running TCP+TLS server
#[cfg(feature = "quic")]
pub struct TcpServerHandle {
    /// Local address
    pub addr: SocketAddr,
    /// Shutdown signal
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    /// Join handle
    join: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "quic")]
impl TcpServerHandle {
    /// Graceful shutdown
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.join.await;
    }
}

/// Events from TCP server (same as QUIC)
pub use crate::quic_data::DataServerEvent as TcpServerEvent;

/// Start a TCP+TLS data server
#[cfg(feature = "quic")]
pub async fn start_tcp_server(
    config: TcpServerConfig,
) -> Result<(TcpServerHandle, mpsc::Receiver<TcpServerEvent>)> {
    // Build TLS acceptor
    let (cert_chain, private_key) = if config.cert_chain.is_empty() {
        let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
        (vec![cert.serialize_der()?], cert.serialize_private_key_der())
    } else {
        (config.cert_chain, config.private_key)
    };

    let key = rustls::pki_types::PrivateKeyDer::try_from(private_key)
        .map_err(|_| anyhow::anyhow!("invalid key"))?;
    let certs: Vec<CertificateDer> = cert_chain.into_iter().map(CertificateDer::from).collect();

    let tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    let acceptor = TlsAcceptor::from(Arc::new(tls_config));

    let listener = TcpListener::bind(config.bind).await?;
    let local_addr = listener.local_addr()?;

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let (event_tx, event_rx) = mpsc::channel::<TcpServerEvent>(64);
    let max_frame_size = config.max_frame_size;

    let join = tokio::spawn(async move {
        let mut client_id_counter: u64 = 0;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, remote_addr) = match accept_result {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    client_id_counter += 1;
                    let client_id = client_id_counter;
                    let acceptor = acceptor.clone();
                    let event_tx = event_tx.clone();

                    tokio::spawn(async move {
                        // TLS handshake
                        let tls_stream = match acceptor.accept(stream).await {
                            Ok(s) => s,
                            Err(e) => {
                                let _ = event_tx.send(TcpServerEvent::Error {
                                    client_id: Some(client_id),
                                    error: format!("TLS handshake failed: {}", e),
                                }).await;
                                return;
                            }
                        };

                        let _ = event_tx.send(TcpServerEvent::ClientConnected {
                            id: client_id,
                            addr: remote_addr,
                        }).await;

                        handle_tcp_client(client_id, tls_stream, max_frame_size, event_tx).await;

                        // Note: ClientDisconnected is sent inside handle_tcp_client
                    });
                }
                _ = &mut shutdown_rx => {
                    break;
                }
            }
        }
    });

    Ok((
        TcpServerHandle {
            addr: local_addr,
            shutdown_tx,
            join,
        },
        event_rx,
    ))
}

/// Handle a connected TCP+TLS client
#[cfg(feature = "quic")]
async fn handle_tcp_client(
    client_id: u64,
    mut stream: ServerTlsStream<TcpStream>,
    max_frame_size: usize,
    event_tx: mpsc::Sender<TcpServerEvent>,
) {
    loop {
        match read_tcp_frame(&mut stream, max_frame_size).await {
            Ok((chan, data)) => {
                match chan {
                    channel::INPUT => {
                        if let Ok(event) = serde_json::from_slice::<InputEvent>(&data) {
                            let _ = event_tx.send(TcpServerEvent::InputReceived {
                                client_id,
                                event,
                            }).await;
                        }
                    }
                    channel::CLIPBOARD => {
                        if let Ok(msg) = serde_json::from_slice::<ClipboardMessage>(&data) {
                            let _ = event_tx.send(TcpServerEvent::ClipboardReceived {
                                client_id,
                                msg,
                            }).await;
                        }
                    }
                    _ => {}
                }
            }
            Err(_) => break,
        }
    }

    let _ = event_tx.send(TcpServerEvent::ClientDisconnected {
        id: client_id,
        reason: "connection closed".into(),
    }).await;
}

// ============================================================================
// Client-Side TCP+TLS
// ============================================================================

/// Configuration for TCP+TLS client
#[derive(Debug, Clone)]
pub struct TcpClientConfig {
    /// Server address
    pub server_addr: SocketAddr,
    /// Server hostname for TLS SNI
    pub server_name: String,
    /// TLS verification mode
    pub tls_mode: TcpTlsMode,
    /// Maximum frame size
    pub max_frame_size: usize,
}

/// TLS verification mode for TCP client
#[derive(Debug, Clone)]
pub enum TcpTlsMode {
    /// Verify against system roots
    System,
    /// Trust any certificate (dev only!)
    Insecure,
}

impl Default for TcpClientConfig {
    fn default() -> Self {
        Self {
            server_addr: SocketAddr::from(([127, 0, 0, 1], 21117)),
            server_name: "localhost".into(),
            tls_mode: TcpTlsMode::Insecure,
            max_frame_size: 4 * 1024 * 1024,
        }
    }
}

/// Handle to a TCP+TLS client connection
#[cfg(feature = "quic")]
pub struct TcpDataClient {
    stream: ClientTlsStream<TcpStream>,
    config: TcpClientConfig,
}

#[cfg(feature = "quic")]
impl TcpDataClient {
    /// Connect to a QuicView server over TCP+TLS
    pub async fn connect(config: TcpClientConfig) -> Result<Self> {
        let connector = build_tcp_tls_connector(&config.tls_mode)?;

        let stream = TcpStream::connect(config.server_addr).await?;
        let server_name = rustls::pki_types::ServerName::try_from(config.server_name.clone())
            .map_err(|_| anyhow::anyhow!("invalid server name"))?;

        let tls_stream = connector.connect(server_name, stream).await?;

        Ok(Self {
            stream: tls_stream,
            config,
        })
    }

    /// Send an input event
    pub async fn send_input(&mut self, event: &InputEvent) -> Result<()> {
        write_tcp_json(&mut self.stream, channel::INPUT, event).await
    }

    /// Send a clipboard message
    pub async fn send_clipboard(&mut self, msg: &ClipboardMessage) -> Result<()> {
        write_tcp_json(&mut self.stream, channel::CLIPBOARD, msg).await
    }

    /// Receive the next frame (blocking)
    /// Returns (channel, data)
    pub async fn recv_frame(&mut self) -> Result<(u8, Bytes)> {
        read_tcp_frame(&mut self.stream, self.config.max_frame_size).await
    }

    /// Receive a screen frame
    pub async fn recv_screen(&mut self) -> Result<ScreenFrame> {
        loop {
            let (chan, data) = self.recv_frame().await?;
            if chan == channel::SCREEN {
                return Ok(serde_json::from_slice(&data)?);
            }
            // Skip non-screen frames
        }
    }

    /// Close the connection
    pub async fn close(mut self) -> Result<()> {
        self.stream.shutdown().await?;
        Ok(())
    }
}

/// Build a TLS connector for the client
#[cfg(feature = "quic")]
fn build_tcp_tls_connector(mode: &TcpTlsMode) -> Result<TlsConnector> {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, SignatureScheme};

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

    let mut roots = rustls::RootCertStore::empty();
    if matches!(mode, TcpTlsMode::System) {
        for cert in rustls_native_certs::load_native_certs()? {
            let _ = roots.add(cert);
        }
    }

    let mut client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    if matches!(mode, TcpTlsMode::Insecure) {
        client_config.dangerous().set_certificate_verifier(Arc::new(InsecureVerifier));
    }

    Ok(TlsConnector::from(Arc::new(client_config)))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(all(test, feature = "quic"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_server_start_stop() {
        let config = TcpServerConfig::default();
        let (handle, _rx) = start_tcp_server(config).await.expect("start server");
        assert!(handle.addr.port() > 0);
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn test_tcp_client_connect_and_input() {
        // Start server
        let server_config = TcpServerConfig {
            bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            ..Default::default()
        };
        let (server_handle, mut events) = start_tcp_server(server_config).await.expect("start server");
        let server_addr = server_handle.addr;

        // Connect client
        let client_config = TcpClientConfig {
            server_addr,
            server_name: "localhost".into(),
            tls_mode: TcpTlsMode::Insecure,
            ..Default::default()
        };
        let mut client = TcpDataClient::connect(client_config).await.expect("connect");

        // Send input event
        client.send_input(&InputEvent::MouseMove { x: 50, y: 100 }).await.expect("send");

        // Server should receive event
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            async {
                loop {
                    if let Some(e) = events.recv().await {
                        match e {
                            TcpServerEvent::InputReceived { event, .. } => return event,
                            _ => continue,
                        }
                    }
                }
            },
        ).await.expect("timeout");

        match event {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, 50);
                assert_eq!(y, 100);
            }
            _ => panic!("unexpected event"),
        }

        let _ = client.close().await;
        server_handle.shutdown().await;
    }
}
