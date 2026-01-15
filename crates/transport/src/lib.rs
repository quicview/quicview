//! Transport crate
//!
//! Provides transport layer implementations for QuicView:
//! - **QUIC control channel**: Signaling, auth, keepalive (existing)
//! - **QUIC data streams**: Screen, input, clipboard over multiplexed QUIC streams
//! - **TCP+TLS fallback**: For UDP-blocked networks

use anyhow::{Context, Result};
use std::net::SocketAddr;

/// QUIC data streams for screen, input, clipboard.
#[cfg(feature = "quic")]
pub mod quic_data;

/// TCP+TLS fallback transport for UDP-blocked networks.
#[cfg(feature = "quic")]
pub mod tcp_data;

/// QUIC control channel API surface (currently a stub scaffold).
pub mod quic_ctrl {
    use super::*;
    #[cfg(feature = "quic")]
    use quinn::{Endpoint, RecvStream, SendStream};
    #[cfg(feature = "quic")]
    use quinn::crypto::rustls as quinn_rustls;
    #[cfg(feature = "quic")]
    use quinn::{ClientConfig as QuinnClientConfig, ServerConfig as QuinnServerConfig};
    #[cfg(feature = "quic")]
    use rustls::{
        client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, ServerName, UnixTime},
        DigitallySignedStruct, SignatureScheme,
    };
    #[cfg(feature = "quic")]
    use std::sync::Arc;
    #[cfg(feature = "quic")]
    use serde::{Deserialize, Serialize};
    #[cfg(all(feature = "quic", feature = "ctrl-proto"))]
    use hbb_common::protobuf::Message;
    
    #[cfg(feature = "quic")]
    use tokio::sync::{mpsc, broadcast, RwLock};
    #[cfg(feature = "quic")]
    use sha2::{Digest, Sha256};
    #[cfg(feature = "quic")]
    use hex::ToHex;
    #[cfg(feature = "quic")]
    use rustls_native_certs as native_certs;
    #[cfg(feature = "quic")]
    use rustls_pemfile as pemfile;

    /// Client-side QUIC control configuration.
    #[cfg(feature = "quic")]
    #[derive(Debug, Clone, Copy)]
    pub struct CtrlClientConfig {
        /// Heartbeat ping interval in seconds.
        pub ping_interval_secs: u64,
        /// Base backoff in milliseconds for reconnects.
        pub backoff_base_ms: u64,
        /// Max backoff cap in milliseconds.
        pub backoff_max_ms: u64,
    }

    #[cfg(feature = "quic")]
    impl Default for CtrlClientConfig {
        fn default() -> Self {
            Self { ping_interval_secs: 10, backoff_base_ms: 500, backoff_max_ms: 32_000 }
        }
    }

    /// Start a local QUIC control listener (stub).
    pub async fn start_echo_server() -> Result<(SocketAddr, tokio::task::JoinHandle<()>)> {
        #[cfg(not(feature = "quic"))]
        {
            anyhow::bail!("quic feature disabled");
        }
        #[cfg(feature = "quic")]
        {
            // Generate a self-signed cert for localhost for dev/testing
            let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
            let cert_der = cert.serialize_der()?;
            let key_der = cert.serialize_private_key_der();

            let key = rustls::pki_types::PrivateKeyDer::try_from(key_der.clone())
                .map_err(|_| anyhow::anyhow!("invalid key"))?;
            let cert_chain = vec![CertificateDer::from(cert_der.clone())];

            let mut server_crypto = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(cert_chain.clone(), key)?;
            server_crypto.alpn_protocols = vec![b"dlnk/ctrl".to_vec()];
            // Wrap rustls config for quinn
            let server_crypto = quinn_rustls::QuicServerConfig::try_from(server_crypto)?;
            let server_config = QuinnServerConfig::with_crypto(Arc::new(server_crypto));

            let bind_addr = SocketAddr::from(([127, 0, 0, 1], 0));
            let endpoint = Endpoint::server(server_config, bind_addr)?;
            let local_addr = endpoint.local_addr()?;

            let join = tokio::spawn(async move {
                loop {
                    match endpoint.accept().await {
                        Some(conn) => {
                            tokio::spawn(async move {
                                if let Ok(connection) = conn.await {
                                    loop {
                                        let bi = match connection.accept_bi().await {
                                            Ok(streams) => streams,
                                            Err(_) => break,
                                        };
                                        let (mut send, mut recv): (SendStream, RecvStream) = bi;
                                        // Echo protocol: read a frame, echo back
                                        let mut buf = Vec::new();
                                        if let Ok(n) = recv.read_to_end(64 * 1024).await {
                                            buf.extend_from_slice(&n);
                                        }
                                        let _ = send.write_all(&buf).await;
                                        let _ = send.finish();
                                    }
                                }
                            });
                        }
                        None => break,
                    }
                }
            });
            Ok((local_addr, join))
        }
    }

    /// Perform a heartbeat (stub).
    pub async fn heartbeat(addr: SocketAddr) -> Result<()> {
        #[cfg(not(feature = "quic"))]
        {
            anyhow::bail!("quic feature disabled");
        }
        #[cfg(feature = "quic")]
        {
            // Insecure verifier that trusts the self-signed cert for dev (do not ship in production)
            #[derive(Debug)]
            struct NoVerify;
            impl ServerCertVerifier for NoVerify {
                fn verify_server_cert(
                    &self,
                    _end_entity: &CertificateDer<'_>,
                    _intermediates: &[CertificateDer<'_>],
                    _server_name: &ServerName<'_>,
                    _ocsp: &[u8],
                    _now: UnixTime,
                ) -> std::result::Result<ServerCertVerified, rustls::Error> {
                    Ok(ServerCertVerified::assertion())
                }
                fn verify_tls12_signature(
                    &self,
                    _message: &[u8],
                    _cert: &CertificateDer<'_>,
                    _dss: &DigitallySignedStruct,
                ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
                    Ok(HandshakeSignatureValid::assertion())
                }
                fn verify_tls13_signature(
                    &self,
                    _message: &[u8],
                    _cert: &CertificateDer<'_>,
                    _dss: &DigitallySignedStruct,
                ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
                    Ok(HandshakeSignatureValid::assertion())
                }
                fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
                    vec![
                        SignatureScheme::ECDSA_NISTP256_SHA256,
                        SignatureScheme::ED25519,
                        SignatureScheme::RSA_PSS_SHA256,
                    ]
                }
            }

            let roots = rustls::RootCertStore::empty();
            // Build client config and install a dangerous verifier for local dev
            let mut client_crypto = rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth();
            client_crypto.alpn_protocols = vec![b"dlnk/ctrl".to_vec()];
            client_crypto
                .dangerous()
                .set_certificate_verifier(Arc::new(NoVerify));
            let client_crypto = quinn_rustls::QuicClientConfig::try_from(client_crypto)?;
            let client_config = QuinnClientConfig::new(Arc::new(client_crypto));

            let mut endpoint = Endpoint::client(SocketAddr::from(([127, 0, 0, 1], 0)))?;
            endpoint.set_default_client_config(client_config);
            let conn = endpoint
                .connect(addr, "localhost")
                .context("connect")?
                .await
                .context("handshake")?;
            let (mut send, mut recv) = conn.open_bi().await?;
            let payload = b"ping";
            send.write_all(payload).await?;
            send.finish()?;
            let echoed = recv.read_to_end(64 * 1024).await?;
            anyhow::ensure!(&echoed == payload, "echo mismatch");
            Ok(())
        }
    }

    // ====== Minimal JSON-framed control protocol ======
    #[cfg(feature = "quic")]
    #[derive(Debug, Serialize, Deserialize, Clone)]
    #[serde(tag = "type", content = "data")]
    pub enum CtrlMessage {
        Hello { agent_id: String, token: String },
        HelloOk,
        HelloErr(String),
        Ping(u64),
        Pong(u64),
        Cmd(Cmd),
        Ack,
        // Auth renewal
        ReauthRequest,
        Reauth { token: String },
        ReauthOk,
        ReauthErr(String),
        // Generic error reporting
        Error { code: String, message: String },
        // Graceful close
        Close { reason: String },
    }

    #[cfg(feature = "quic")]
    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub enum Cmd { Start, Stop }

    #[cfg(feature = "quic")]
    #[derive(Debug)]
    pub enum CtrlEvent { Liveness, Command(Cmd), Connected, Disconnected(String), AuthRenewed, Error(String) }

    #[cfg(feature = "quic")]
    async fn write_frame(send: &mut SendStream, msg: &CtrlMessage) -> Result<()> {
        #[cfg(feature = "ctrl-proto")]
        {
            // Encode via protobuf envelope for ctrl-proto feature
            let env = to_proto_envelope(msg.clone());
            let mut payload = Vec::new();
            env.write_to_vec(&mut payload)?;
            let len = (payload.len() as u32).to_be_bytes();
            send.write_all(&len).await?;
            send.write_all(&payload).await?;
            return Ok(());
        }
        #[cfg(not(feature = "ctrl-proto"))]
        {
            let payload = serde_json::to_vec(msg)?;
            let len = (payload.len() as u32).to_be_bytes();
            send.write_all(&len).await?;
            send.write_all(&payload).await?;
            Ok(())
        }
    }

    #[cfg(feature = "quic")]
    async fn read_frame(recv: &mut RecvStream) -> Result<CtrlMessage> {
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf).await?;
        #[cfg(feature = "ctrl-proto")]
        {
            let env = hbb_common::control_proto::ControlEnvelope::parse_from_bytes(&buf)?;
            return from_proto_envelope(env);
        }
        #[cfg(not(feature = "ctrl-proto"))]
        {
            let msg: CtrlMessage = serde_json::from_slice(&buf)?;
            Ok(msg)
        }
    }

    // ===== Protobuf envelope conversions (feature-gated) =====
    #[cfg(all(feature = "quic", feature = "ctrl-proto"))]
    fn to_proto_envelope(msg: CtrlMessage) -> hbb_common::control_proto::ControlEnvelope {
        use hbb_common::control_proto as pb;
        let mut env = pb::ControlEnvelope::new();
        env.protocol_version = "1".to_string();
        match msg {
            CtrlMessage::Hello { agent_id, token } => {
                let mut h = pb::ControlHello::new();
                h.agent_id = agent_id;
                h.token = token;
                h.version = "1".into();
                env.union = Some(pb::control_envelope::Union::Hello(h));
            }
            CtrlMessage::HelloOk => {
                env.union = Some(pb::control_envelope::Union::HelloOk(pb::ControlHelloOk::new()));
            }
            CtrlMessage::HelloErr(reason) => {
                let mut e = pb::ControlHelloErr::new();
                e.reason = reason;
                env.union = Some(pb::control_envelope::Union::HelloErr(e));
            }
            CtrlMessage::Ping(n) => {
                let mut p = pb::ControlPing::new(); p.nonce = n; env.union = Some(pb::control_envelope::Union::Ping(p));
            }
            CtrlMessage::Pong(n) => {
                let mut p = pb::ControlPong::new(); p.nonce = n; env.union = Some(pb::control_envelope::Union::Pong(p));
            }
            CtrlMessage::Cmd(c) => {
                let mut cmd = pb::ControlCmd::new();
                cmd.cmd = match c { Cmd::Start => pb::ControlCmdType::CMD_START, Cmd::Stop => pb::ControlCmdType::CMD_STOP }.into();
                env.union = Some(pb::control_envelope::Union::Cmd(cmd));
            }
            CtrlMessage::Ack => {
                env.union = Some(pb::control_envelope::Union::Ack(pb::ControlAck::new()));
            }
            CtrlMessage::ReauthRequest => {
                env.union = Some(pb::control_envelope::Union::ReauthRequest(pb::ControlReauthRequest::new()));
            }
            CtrlMessage::Reauth { token } => {
                let mut r = pb::ControlReauth::new(); r.token = token; env.union = Some(pb::control_envelope::Union::Reauth(r));
            }
            CtrlMessage::ReauthOk => {
                env.union = Some(pb::control_envelope::Union::ReauthOk(pb::ControlReauthOk::new()));
            }
            CtrlMessage::ReauthErr(reason) => {
                let mut r = pb::ControlReauthErr::new(); r.reason = reason; env.union = Some(pb::control_envelope::Union::ReauthErr(r));
            }
            CtrlMessage::Error { code, message } => {
                let mut err = pb::ControlError::new();
                err.code = code;
                err.message = message;
                env.union = Some(pb::control_envelope::Union::Error(err));
            }
            CtrlMessage::Close { reason } => {
                let mut c = pb::ControlClose::new(); c.reason = reason; env.union = Some(pb::control_envelope::Union::Close(c));
            }
        }
        env
    }

    #[cfg(all(feature = "quic", feature = "ctrl-proto"))]
    fn from_proto_envelope(env: hbb_common::control_proto::ControlEnvelope) -> Result<CtrlMessage> {
        use hbb_common::control_proto as pb;
        use pb::control_envelope::Union::*;
        let Some(u) = env.union else { anyhow::bail!("empty ctrl envelope") };
        let msg = match u {
            Hello(h) => CtrlMessage::Hello { agent_id: h.agent_id, token: h.token },
            HelloOk(_) => CtrlMessage::HelloOk,
            HelloErr(e) => CtrlMessage::HelloErr(e.reason),
            Ping(p) => CtrlMessage::Ping(p.nonce),
            Pong(p) => CtrlMessage::Pong(p.nonce),
            Cmd(c) => {
                let which = c.cmd.enum_value_or(pb::ControlCmdType::CMD_STOP);
                let cmd = match which { pb::ControlCmdType::CMD_START => self::Cmd::Start, _ => self::Cmd::Stop };
                CtrlMessage::Cmd(cmd)
            }
            Ack(_) => CtrlMessage::Ack,
            ReauthRequest(_) => CtrlMessage::ReauthRequest,
            Reauth(r) => CtrlMessage::Reauth { token: r.token },
            ReauthOk(_) => CtrlMessage::ReauthOk,
            ReauthErr(e) => CtrlMessage::ReauthErr(e.reason),
            Close(c) => CtrlMessage::Close { reason: c.reason },
            Error(e) => CtrlMessage::Error { code: e.code, message: e.message },
            _ => CtrlMessage::Error { code: "unknown".into(), message: "unhandled control variant".into() },
        };
        Ok(msg)
    }

    /// Start a control server that validates Hello token and can send commands via returned channel.
    #[cfg(feature = "quic")]
    #[derive(Debug, Clone)]
    pub enum ServerSignal { ReauthRequest, UpdateToken(String), Close(String), Error { code: String, message: String } }

    pub async fn start_ctrl_server(bind: SocketAddr, expected_token: String) -> Result<(SocketAddr, tokio::task::JoinHandle<()>, broadcast::Sender<Cmd>, broadcast::Sender<ServerSignal>)> {
        #[cfg(not(feature = "quic"))]
        {
            anyhow::bail!("quic feature disabled");
        }
        #[cfg(feature = "quic")]
        {
            // Self-signed localhost cert
            let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
            let cert_der = cert.serialize_der()?;
            let key_der = cert.serialize_private_key_der();
            let key = rustls::pki_types::PrivateKeyDer::try_from(key_der.clone())
                .map_err(|_| anyhow::anyhow!("invalid key"))?;
            let cert_chain = vec![CertificateDer::from(cert_der.clone())];
            let mut server_crypto = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(cert_chain, key)?;
            server_crypto.alpn_protocols = vec![b"dlnk/ctrl".to_vec()];
            let server_crypto = quinn_rustls::QuicServerConfig::try_from(server_crypto)?;
            let server_config = QuinnServerConfig::with_crypto(Arc::new(server_crypto));
            let endpoint = Endpoint::server(server_config, bind)?;
            let local_addr = endpoint.local_addr()?;
            // Commands injected by tests or external control
            let (tx_cmd, _) = broadcast::channel::<Cmd>(8);
            // Signals to inject server-initiated actions (e.g., reauth request)
            let (tx_sig, _) = broadcast::channel::<ServerSignal>(8);
            // Expected token can change after reauth
            let expected_token = Arc::new(RwLock::new(expected_token));
            let tx_cmd_for_task = tx_cmd.clone();
            let tx_sig_for_task = tx_sig.clone();
            let join = tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    // Handle each connection
                    let expected_token = expected_token.clone();
                    let mut rx_cmd = tx_cmd_for_task.subscribe();
                    let mut rx_sig = tx_sig_for_task.subscribe();
                    tokio::spawn(async move {
                        let conn = match incoming.await { Ok(c) => c, Err(_) => return };
                        // Accept a BI stream
                        let (mut send, mut recv) = match conn.accept_bi().await { Ok(s) => s, Err(_) => return };
                        // Handshake
                        let hello = match read_frame(&mut recv).await { Ok(m) => m, Err(_) => return };
                        match hello {
                            CtrlMessage::Hello { token, .. } if token == *expected_token.read().await => {
                                // Auth ok
                                if write_frame(&mut send, &CtrlMessage::HelloOk).await.is_err() { return; }
                            }
                            _ => {
                                let _ = write_frame(&mut send, &CtrlMessage::HelloErr("auth".into())).await;
                                return;
                            }
                        }

                        // Main loop: respond to Ping, forward queued commands
                        let mut ping_nonce: u64 = 0;
                        loop {
                            tokio::select! {
                                incoming = read_frame(&mut recv) => {
                                    match incoming {
                                        Ok(CtrlMessage::Ping(n)) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Pong(n)).await;
                                        }
                                        Ok(CtrlMessage::Reauth { token }) => {
                                            // Update expected token and ack
                                            *expected_token.write().await = token;
                                            let _ = write_frame(&mut send, &CtrlMessage::ReauthOk).await;
                                        }
                                        Ok(CtrlMessage::Close { .. }) => {
                                            // Client requested close; end loop
                                            break;
                                        }
                                        Ok(_) => {}
                                        Err(_) => break,
                                    }
                                }
                                maybe_cmd = rx_cmd.recv() => {
                                    match maybe_cmd {
                                        Ok(cmd) => {
                                            if write_frame(&mut send, &CtrlMessage::Cmd(cmd)).await.is_err() { break; }
                                        }
                                        Err(_) => { break; }
                                    }
                                }
                                maybe_sig = rx_sig.recv() => {
                                    match maybe_sig {
                                        Ok(ServerSignal::ReauthRequest) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::ReauthRequest).await;
                                        }
                                        Ok(ServerSignal::UpdateToken(new_tok)) => {
                                            *expected_token.write().await = new_tok;
                                            // Optionally notify client to reauth with the new token
                                            let _ = write_frame(&mut send, &CtrlMessage::ReauthRequest).await;
                                        }
                                        Ok(ServerSignal::Close(reason)) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Close { reason }).await;
                                            break;
                                        }
                                        Ok(ServerSignal::Error { code, message }) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Error { code, message }).await;
                                        }
                                        Err(_) => { break; }
                                    }
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                                    // Idle keepalive ping; ignore errors
                                    ping_nonce = ping_nonce.wrapping_add(1);
                                    if write_frame(&mut send, &CtrlMessage::Ping(ping_nonce)).await.is_err() { break; }
                                }
                            }
                        }
                    });
                }
            });
            Ok((local_addr, join, tx_cmd, tx_sig))
        }
    }

    /// Start a control server and also return its leaf certificate DER (for pin/TOFU tests).
    pub async fn start_ctrl_server_with_cert(bind: SocketAddr, expected_token: String) -> Result<(SocketAddr, tokio::task::JoinHandle<()>, broadcast::Sender<Cmd>, broadcast::Sender<ServerSignal>, Vec<u8>)> {
        #[cfg(not(feature = "quic"))]
        {
            anyhow::bail!("quic feature disabled");
        }
        #[cfg(feature = "quic")]
        {
            // Self-signed localhost cert
            let cert = rcgen::generate_simple_self_signed(["localhost".to_string()])?;
            let cert_der = cert.serialize_der()?;
            let key_der = cert.serialize_private_key_der();
            let key = rustls::pki_types::PrivateKeyDer::try_from(key_der.clone())
                .map_err(|_| anyhow::anyhow!("invalid key"))?;
            let cert_chain = vec![CertificateDer::from(cert_der.clone())];
            let mut server_crypto = rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(cert_chain, key)?;
            server_crypto.alpn_protocols = vec![b"dlnk/ctrl".to_vec()];
            let server_crypto = quinn_rustls::QuicServerConfig::try_from(server_crypto)?;
            let server_config = QuinnServerConfig::with_crypto(Arc::new(server_crypto));
            let endpoint = Endpoint::server(server_config, bind)?;
            let local_addr = endpoint.local_addr()?;
            // Commands injected by tests or external control
            let (tx_cmd, _) = broadcast::channel::<Cmd>(8);
            // Signals to inject server-initiated actions (e.g., reauth request)
            let (tx_sig, _) = broadcast::channel::<ServerSignal>(8);
            // Expected token can change after reauth
            let expected_token = Arc::new(RwLock::new(expected_token));
            let tx_cmd_for_task = tx_cmd.clone();
            let tx_sig_for_task = tx_sig.clone();
            let join = tokio::spawn(async move {
                while let Some(incoming) = endpoint.accept().await {
                    // Handle each connection
                    let expected_token = expected_token.clone();
                    let mut rx_cmd = tx_cmd_for_task.subscribe();
                    let mut rx_sig = tx_sig_for_task.subscribe();
                    tokio::spawn(async move {
                        let conn = match incoming.await { Ok(c) => c, Err(_) => return };
                        // Accept a BI stream
                        let (mut send, mut recv) = match conn.accept_bi().await { Ok(s) => s, Err(_) => return };
                        // Handshake
                        let hello = match read_frame(&mut recv).await { Ok(m) => m, Err(_) => return };
                        match hello {
                            CtrlMessage::Hello { token, .. } if token == *expected_token.read().await => {
                                // Auth ok
                                if write_frame(&mut send, &CtrlMessage::HelloOk).await.is_err() { return; }
                            }
                            _ => {
                                let _ = write_frame(&mut send, &CtrlMessage::HelloErr("auth".into())).await;
                                return;
                            }
                        }

                        // Main loop: respond to Ping, forward queued commands
                        let mut ping_nonce: u64 = 0;
                        loop {
                            tokio::select! {
                                incoming = read_frame(&mut recv) => {
                                    match incoming {
                                        Ok(CtrlMessage::Ping(n)) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Pong(n)).await;
                                        }
                                        Ok(CtrlMessage::Reauth { token }) => {
                                            // Update expected token and ack
                                            *expected_token.write().await = token;
                                            let _ = write_frame(&mut send, &CtrlMessage::ReauthOk).await;
                                        }
                                        Ok(CtrlMessage::Close { .. }) => { break; }
                                        Ok(_) => {}
                                        Err(_) => break,
                                    }
                                }
                                maybe_cmd = rx_cmd.recv() => {
                                    match maybe_cmd {
                                        Ok(cmd) => {
                                            if write_frame(&mut send, &CtrlMessage::Cmd(cmd)).await.is_err() { break; }
                                        }
                                        Err(_) => { break; }
                                    }
                                }
                                maybe_sig = rx_sig.recv() => {
                                    match maybe_sig {
                                        Ok(ServerSignal::ReauthRequest) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::ReauthRequest).await;
                                        }
                                        Ok(ServerSignal::UpdateToken(new_tok)) => {
                                            *expected_token.write().await = new_tok;
                                            // Optionally notify client to reauth with the new token
                                            let _ = write_frame(&mut send, &CtrlMessage::ReauthRequest).await;
                                        }
                                        Ok(ServerSignal::Close(reason)) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Close { reason }).await;
                                            break;
                                        }
                                        Ok(ServerSignal::Error { code, message }) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Error { code, message }).await;
                                        }
                                        Err(_) => { break; }
                                    }
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                                    // Idle keepalive ping; ignore errors
                                    ping_nonce = ping_nonce.wrapping_add(1);
                                    if write_frame(&mut send, &CtrlMessage::Ping(ping_nonce)).await.is_err() { break; }
                                }
                            }
                        }
                    });
                }
            });
            Ok((local_addr, join, tx_cmd, tx_sig, cert_der))
        }
    }

    /// Run a control client that connects, authenticates, handles pings, and emits events.
    #[cfg(feature = "quic")]
    pub async fn run_ctrl_client(addr: SocketAddr, token: String, cfg: CtrlClientConfig) -> Result<(tokio::task::JoinHandle<()>, mpsc::Receiver<CtrlEvent>)> {
    // Backward-compat shim: default to insecure (dev only). Prefer run_ctrl_client_with_tls.
    let client_config = build_client_config_tls(TlsMode::InsecureNoVerify, None, None)?;

        async fn connect_once(addr: SocketAddr, client_config: QuinnClientConfig, token: &str) -> Result<(quinn::Connection, SendStream, RecvStream)> {
            let mut endpoint = Endpoint::client(SocketAddr::from(([127,0,0,1],0)))?;
            endpoint.set_default_client_config(client_config);
            let conn = endpoint.connect(addr, "localhost").context("connect")?.await.context("handshake")?;
            let (mut send, mut recv) = conn.open_bi().await?;
            let hello = CtrlMessage::Hello { agent_id: "agent-1".into(), token: token.to_string() };
            write_frame(&mut send, &hello).await?;
            match read_frame(&mut recv).await? {
                CtrlMessage::HelloOk => {}
                CtrlMessage::HelloErr(e) => anyhow::bail!("auth: {}", e),
                _ => anyhow::bail!("unexpected handshake reply"),
            }
            Ok((conn, send, recv))
        }

        // Event channel
    let (tx_evt, rx_evt) = mpsc::channel::<CtrlEvent>(32);
        let join = tokio::spawn(async move {
            // Reconnect loop with backoff
            let mut attempt: u32 = 0;
            let token_current = token.clone();
            loop {
                match connect_once(addr, client_config.clone(), &token_current).await {
                    Ok((_conn, mut send, mut recv)) => {
                        attempt = 0;
                        let _ = tx_evt.send(CtrlEvent::Connected).await;
                        let mut nonce: u64 = 0;
                        let mut awaiting_reauth = false;
                        loop {
                            tokio::select! {
                                incoming = read_frame(&mut recv) => {
                                    match incoming {
                                        Ok(CtrlMessage::Ping(n)) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Pong(n)).await;
                                            let _ = tx_evt.send(CtrlEvent::Liveness).await;
                                        }
                                        Ok(CtrlMessage::Error { code, message }) => {
                                            let _ = tx_evt.send(CtrlEvent::Error(format!("{}: {}", code, message))).await;
                                        }
                                        Ok(CtrlMessage::Cmd(cmd)) => {
                                            let _ = tx_evt.send(CtrlEvent::Command(cmd)).await;
                                        }
                                        Ok(CtrlMessage::ReauthRequest) => {
                                            // Re-send current token
                                            let _ = write_frame(&mut send, &CtrlMessage::Reauth { token: token_current.clone() }).await;
                                            awaiting_reauth = true;
                                        }
                                        Ok(CtrlMessage::ReauthOk) => {
                                            if awaiting_reauth {
                                                awaiting_reauth = false;
                                                let _ = tx_evt.send(CtrlEvent::AuthRenewed).await;
                                            }
                                        }
                                        Ok(_) => {}
                                        Err(e) => {
                                            let _ = tx_evt.send(CtrlEvent::Disconnected(format!("{}", e))).await; break;
                                        }
                                    }
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_secs(cfg.ping_interval_secs)) => {
                                    nonce = nonce.wrapping_add(1);
                                    let _ = write_frame(&mut send, &CtrlMessage::Ping(nonce)).await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        attempt = attempt.saturating_add(1);
                        let _ = tx_evt.send(CtrlEvent::Disconnected(format!("{}", e))).await;
                    }
                }
                // Backoff before next reconnect attempt
                let base_ms = cfg.backoff_base_ms.max(1);
                let cap_ms = cfg.backoff_max_ms.max(base_ms);
                let pow = 1u64 << (attempt.min(20) as u32); // safe bounded shift
                let delay = base_ms.saturating_mul(pow).min(cap_ms);
                // add jitter up to 250ms or 1/2 base, whichever smaller
                let jitter_cap = std::cmp::min(250, (base_ms / 2) as usize) as u64;
                let jitter = if jitter_cap > 0 { (tokio::time::Instant::now().elapsed().as_nanos() as u64) % jitter_cap } else { 0 };
                tokio::time::sleep(std::time::Duration::from_millis(delay + jitter)).await;
            }
        });
        Ok((join, rx_evt))
    }

    #[cfg(feature = "quic")]
    #[derive(Clone)]
    pub enum TlsMode {
        // Verify against system roots and optional extra CA
        SystemRoots { sni: String, ca_pem: Option<Vec<u8>> },
        // Pin a specific leaf cert SHA256 (hex digest of DER)
        PinSha256 { sni: String, der_sha256_hex: String },
        // Trust-on-first-use: accept first cert and pin its SHA256 via a callback
        Tofu { sni: String, on_first: Arc<dyn Fn(String) + Send + Sync> },
        // Insecure: no verification (dev only)
        InsecureNoVerify,
    }
    impl std::fmt::Debug for TlsMode {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TlsMode::SystemRoots { sni, ca_pem } => f.debug_struct("SystemRoots").field("sni", sni).field("ca_pem", &ca_pem.as_ref().map(|v| v.len())).finish(),
                TlsMode::PinSha256 { sni, der_sha256_hex } => f.debug_struct("PinSha256").field("sni", sni).field("der_sha256_hex", der_sha256_hex).finish(),
                TlsMode::Tofu { sni, .. } => f.debug_struct("Tofu").field("sni", sni).finish(),
                TlsMode::InsecureNoVerify => write!(f, "InsecureNoVerify"),
            }
        }
    }

    #[cfg(feature = "quic")]
    fn build_client_config_tls(mode: TlsMode, cached_pin: Option<String>, cached_ca: Option<Vec<u8>>) -> Result<QuinnClientConfig> {
        // Helper verifiers
        #[derive(Debug)]
        struct NoVerify;
        impl ServerCertVerifier for NoVerify {
            fn verify_server_cert(&self, _end_entity: &CertificateDer<'_>, _intermediates: &[CertificateDer<'_>], _server_name: &ServerName<'_>, _ocsp: &[u8], _now: UnixTime,) -> std::result::Result<ServerCertVerified, rustls::Error> { Ok(ServerCertVerified::assertion()) }
            fn verify_tls12_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &DigitallySignedStruct,) -> std::result::Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
            fn verify_tls13_signature(&self, _message: &[u8], _cert: &CertificateDer<'_>, _dss: &DigitallySignedStruct,) -> std::result::Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256] }
        }

    #[derive(Debug)]
    struct PinVerifier { sha256_hex: String }
        impl ServerCertVerifier for PinVerifier {
            fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _intermediates: &[CertificateDer<'_>], _server_name: &ServerName<'_>, _ocsp: &[u8], _now: UnixTime,) -> std::result::Result<ServerCertVerified, rustls::Error> {
                let mut hasher = Sha256::new();
                hasher.update(end_entity.as_ref());
                let got = hasher.finalize().encode_hex::<String>();
                if got.eq_ignore_ascii_case(&self.sha256_hex) {
                    Ok(ServerCertVerified::assertion())
                } else {
                    Err(rustls::Error::General("pinned cert mismatch".into()))
                }
            }
            fn verify_tls12_signature(&self, _m: &[u8], _c: &CertificateDer<'_>, _d: &DigitallySignedStruct,) -> std::result::Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
            fn verify_tls13_signature(&self, _m: &[u8], _c: &CertificateDer<'_>, _d: &DigitallySignedStruct,) -> std::result::Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256] }
        }

        struct TofuVerifier { sni: String, first_cb: Arc<dyn Fn(String) + Send + Sync>, cached: Option<String> }
        impl std::fmt::Debug for TofuVerifier {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("TofuVerifier").field("sni", &self.sni).field("cached", &self.cached).finish()
            }
        }
        impl ServerCertVerifier for TofuVerifier {
            fn verify_server_cert(&self, end_entity: &CertificateDer<'_>, _intermediates: &[CertificateDer<'_>], _server_name: &ServerName<'_>, _ocsp: &[u8], _now: UnixTime,) -> std::result::Result<ServerCertVerified, rustls::Error> {
                let mut hasher = Sha256::new();
                hasher.update(end_entity.as_ref());
                let got = hasher.finalize().encode_hex::<String>();
                if let Some(expected) = &self.cached {
                    if got.eq_ignore_ascii_case(expected) {
                        return Ok(ServerCertVerified::assertion());
                    }
                    return Err(rustls::Error::General("TOFU pin mismatch".into()));
                }
                // First use: accept and emit pin via callback
                (self.first_cb)(got);
                Ok(ServerCertVerified::assertion())
            }
            fn verify_tls12_signature(&self, _m: &[u8], _c: &CertificateDer<'_>, _d: &DigitallySignedStruct,) -> std::result::Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
            fn verify_tls13_signature(&self, _m: &[u8], _c: &CertificateDer<'_>, _d: &DigitallySignedStruct,) -> std::result::Result<HandshakeSignatureValid, rustls::Error> { Ok(HandshakeSignatureValid::assertion()) }
            fn supported_verify_schemes(&self) -> Vec<SignatureScheme> { vec![SignatureScheme::ECDSA_NISTP256_SHA256, SignatureScheme::ED25519, SignatureScheme::RSA_PSS_SHA256] }
        }

        let mut client_crypto = match &mode {
            TlsMode::SystemRoots { .. } | TlsMode::PinSha256 { .. } | TlsMode::Tofu { .. } => {
                let mut roots = rustls::RootCertStore::empty();
                // Load platform roots
                for cert in native_certs::load_native_certs().map_err(|_| anyhow::anyhow!("load system roots"))? {
                    let _ = roots.add(cert);
                }
                // Append optional extra CA (either param or cached_ca from disk)
                if let Some(pem) = cached_ca.as_ref().or_else(|| match &mode { TlsMode::SystemRoots { ca_pem: Some(p), .. } => Some(p), _ => None }) {
                    let mut rd = std::io::BufReader::new(&pem[..]);
                    for item in pemfile::certs(&mut rd) {
                        if let Ok(c) = item { let _ = roots.add(c); }
                    }
                }
                rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()
            }
            TlsMode::InsecureNoVerify => {
                let roots = rustls::RootCertStore::empty();
                rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()
            }
        };
        client_crypto.alpn_protocols = vec![b"dlnk/ctrl".to_vec()];
        match mode {
            TlsMode::SystemRoots { .. } => {}
            TlsMode::PinSha256 { der_sha256_hex, .. } => {
                client_crypto.dangerous().set_certificate_verifier(Arc::new(PinVerifier { sha256_hex: der_sha256_hex }));
            }
            TlsMode::Tofu { sni: _, on_first, .. } => {
                client_crypto.dangerous().set_certificate_verifier(Arc::new(TofuVerifier { sni: String::new(), first_cb: on_first, cached: cached_pin }));
            }
            TlsMode::InsecureNoVerify => {
                client_crypto.dangerous().set_certificate_verifier(Arc::new(NoVerify));
            }
        }
        let client_crypto = quinn_rustls::QuicClientConfig::try_from(client_crypto)?;
        Ok(QuinnClientConfig::new(Arc::new(client_crypto)))
    }

    #[cfg(feature = "quic")]
    pub async fn run_ctrl_client_with_tls(addr: SocketAddr, token: String, cfg: CtrlClientConfig, tls: TlsMode, cached_pin: Option<String>, cached_ca: Option<Vec<u8>>) -> Result<(tokio::task::JoinHandle<()>, mpsc::Receiver<CtrlEvent>)> {
        let client_config = build_client_config_tls(tls, cached_pin, cached_ca)?;
        // Reuse the rest of run_ctrl_client logic by inlining the connect loop here
        async fn connect_once(addr: SocketAddr, client_config: QuinnClientConfig, token: &str) -> Result<(quinn::Connection, SendStream, RecvStream)> {
            let mut endpoint = Endpoint::client(SocketAddr::from(([127,0,0,1],0)))?;
            endpoint.set_default_client_config(client_config);
            let conn = endpoint.connect(addr, "localhost").context("connect")?.await.context("handshake")?;
            let (mut send, mut recv) = conn.open_bi().await?;
            let hello = CtrlMessage::Hello { agent_id: "agent-1".into(), token: token.to_string() };
            write_frame(&mut send, &hello).await?;
            match read_frame(&mut recv).await? {
                CtrlMessage::HelloOk => {}
                CtrlMessage::HelloErr(e) => anyhow::bail!("auth: {}", e),
                _ => anyhow::bail!("unexpected handshake reply"),
            }
            Ok((conn, send, recv))
        }
        let (tx_evt, rx_evt) = mpsc::channel::<CtrlEvent>(32);
        let join = tokio::spawn(async move {
            let mut attempt: u32 = 0;
            let token_current = token.clone();
            loop {
                match connect_once(addr, client_config.clone(), &token_current).await {
                    Ok((_conn, mut send, mut recv)) => {
                        attempt = 0;
                        let _ = tx_evt.send(CtrlEvent::Connected).await;
                        let mut nonce: u64 = 0;
                        let mut awaiting_reauth = false;
                        loop {
                            tokio::select! {
                                incoming = read_frame(&mut recv) => {
                                    match incoming {
                                        Ok(CtrlMessage::Ping(n)) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Pong(n)).await;
                                            let _ = tx_evt.send(CtrlEvent::Liveness).await;
                                        }
                                        Ok(CtrlMessage::Error { code, message }) => {
                                            let _ = tx_evt.send(CtrlEvent::Error(format!("{}: {}", code, message))).await;
                                        }
                                        Ok(CtrlMessage::Cmd(cmd)) => {
                                            let _ = tx_evt.send(CtrlEvent::Command(cmd)).await;
                                        }
                                        Ok(CtrlMessage::ReauthRequest) => {
                                            let _ = write_frame(&mut send, &CtrlMessage::Reauth { token: token_current.clone() }).await;
                                            awaiting_reauth = true;
                                        }
                                        Ok(CtrlMessage::ReauthOk) => {
                                            if awaiting_reauth {
                                                awaiting_reauth = false;
                                                let _ = tx_evt.send(CtrlEvent::AuthRenewed).await;
                                            }
                                        }
                                        Ok(_) => {}
                                        Err(e) => { let _ = tx_evt.send(CtrlEvent::Disconnected(format!("{}", e))).await; break; }
                                    }
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_secs(cfg.ping_interval_secs)) => {
                                    nonce = nonce.wrapping_add(1);
                                    let _ = write_frame(&mut send, &CtrlMessage::Ping(nonce)).await;
                                }
                            }
                        }
                    }
                    Err(e) => { attempt = attempt.saturating_add(1); let _ = tx_evt.send(CtrlEvent::Disconnected(format!("{}", e))).await; }
                }
                let base_ms = cfg.backoff_base_ms.max(1);
                let cap_ms = cfg.backoff_max_ms.max(base_ms);
                let pow = 1u64 << (attempt.min(20) as u32);
                let delay = base_ms.saturating_mul(pow).min(cap_ms);
                let jitter_cap = std::cmp::min(250, (base_ms / 2) as usize) as u64;
                let jitter = if jitter_cap > 0 { (tokio::time::Instant::now().elapsed().as_nanos() as u64) % jitter_cap } else { 0 };
                tokio::time::sleep(std::time::Duration::from_millis(delay + jitter)).await;
            }
        });
        Ok((join, rx_evt))
    }
}

#[cfg(all(test, feature = "quic"))]
mod tests {
    use super::quic_ctrl;
    use super::*;
    use sha2::{Digest, Sha256};
    use hex::ToHex;
    // no extra rustls imports needed here

    #[tokio::test]
    async fn quic_echo_heartbeat_smoke() {
        let (addr, handle) = quic_ctrl::start_echo_server().await.expect("start server");
        quic_ctrl::heartbeat(addr).await.expect("heartbeat ok");
        // Drop the server by aborting task; ensure it stops without panicking
        handle.abort();
    }

    // Helper to compute a pin from DER bytes
    fn pin_from_der(der: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(der);
        hasher.finalize().encode_hex::<String>()
    }

    #[tokio::test]
    async fn quic_ctrl_tls_pin_and_tofu() {
        // Start control server with a token
        let bind = SocketAddr::from(([127,0,0,1], 0));
    let (addr, join, _tx_cmd, _tx_sig, cert_der) = quic_ctrl::start_ctrl_server_with_cert(bind, "tok".into()).await.unwrap();
    // Compute pin from server's DER
    let pin = pin_from_der(&cert_der);
        assert!(!pin.is_empty());
        // Connect with pin mode
        let tls = quic_ctrl::TlsMode::PinSha256 { sni: "localhost".into(), der_sha256_hex: pin.clone() };
        let cfg = quic_ctrl::CtrlClientConfig::default();
        let (_h, mut rx) = quic_ctrl::run_ctrl_client_with_tls(addr, "tok".into(), cfg, tls, None, None).await.unwrap();
        // Expect a Connected event soon
        let ev = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
        match ev { quic_ctrl::CtrlEvent::Connected => {}, other => panic!("unexpected: {:?}", other) }
        // TOFU: first run should accept and invoke callback
        let seen = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
        let seen2 = seen.clone();
        let tls2 = quic_ctrl::TlsMode::Tofu { sni: "localhost".into(), on_first: std::sync::Arc::new(move |p: String| { *seen2.lock().unwrap() = Some(p); }) };
        let (_h2, mut rx2) = quic_ctrl::run_ctrl_client_with_tls(addr, "tok".into(), cfg, tls2, None, None).await.unwrap();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), rx2.recv()).await.unwrap().unwrap();
        let learned = seen.lock().unwrap().clone();
        assert_eq!(learned.unwrap().to_lowercase(), pin.to_lowercase());
        // Cleanup
        join.abort();
    }
}
