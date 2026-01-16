//! QuicView client crate
//! - Headless client core (`core` module) for UI-agnostic operation
//! - QUIC client for direct server connection (`quic_client` module)
//! - Legacy `ClientLauncher` stub retained for backward compatibility

use bridge::{BridgeError, ClientLauncher};
use config::QuicViewConfig;

mod capture;

/// QUIC client for connecting to a QuicView server.
/// Provides screen frame reception, input event sending, and clipboard sync.
pub mod quic_client;

pub use quic_client::{
    ClientStats, ClientTlsMode, MouseButtonType, QuicClientConfig, QuicClientError,
    QuicClientEvent, QuicViewClient, ScreenEncodingType, ScreenFrameEvent,
};

/// Legacy launcher stub (Flutter integration removed). Returns NotImplemented for now.
pub struct Client;

impl ClientLauncher for Client {
    fn launch_client(&self, cfg: &QuicViewConfig) -> Result<(), BridgeError> {
        let _ = cfg; // unused for now
        Err(BridgeError::NotImplemented)
    }
}

/// Headless client core exposing an event stream for UI layers (Leptos/Tauri to consume later).
pub mod core {
    use std::{sync::Arc, time::Duration};
    use thiserror::Error;
    use tokio::{sync::{broadcast, RwLock}, task::JoinHandle};
    use transport::quic_ctrl::{self, CtrlEvent, Cmd, CtrlClientConfig};

    #[derive(Debug, Error)]
    pub enum ClientError {
        #[error("already running")] AlreadyRunning,
        #[error("not running")] NotRunning,
    }

    #[derive(Debug, Clone)]
    pub enum ClientEvent {
        Started,
        Stopped,
        SessionIncoming { peer_id: String },
        ConsentRequired { reason: &'static str },
        CtrlConnected,
        CtrlDisconnected { reason: String },
        CtrlAuthRenewed,
    }

    #[derive(Default)]
    struct State {
        running: bool,
        // QUIC control channel status
        ctrl_connected: bool,
        last_disconnect: Option<String>,
        reconnects: u64,
        last_connected_at: Option<u64>,
        last_disconnect_at: Option<u64>,
        last_error: Option<String>,
        last_attempt_at: Option<u64>,
        attempts: u64,
        // Active ctrl tunables (if control channel was started)
        ctrl_ping_secs: Option<u64>,
        ctrl_backoff_base_ms: Option<u64>,
        ctrl_backoff_max_ms: Option<u64>,
        // TLS trust state
        tls: Option<CtrlTlsSnapshot>,
    }

    /// Snapshot of QUIC control channel status for UI/status reporting.
    #[cfg_attr(feature = "http-ui", derive(serde::Serialize))]
    #[derive(Debug, Clone)]
    pub struct CtrlSnapshot {
        pub connected: bool,
        pub last_disconnect: Option<String>,
        pub reconnects: u64,
        pub last_connected_at: Option<u64>,
        pub last_disconnect_at: Option<u64>,
        pub last_error: Option<String>,
        pub last_attempt_at: Option<u64>,
        pub attempts: u64,
        pub ping_interval_secs: Option<u64>,
        pub backoff_base_ms: Option<u64>,
        pub backoff_max_ms: Option<u64>,
        /// TLS trust snapshot
        pub tls: Option<CtrlTlsSnapshot>,
    }

    /// Snapshot of TLS trust configuration for the control channel.
    #[cfg_attr(feature = "http-ui", derive(serde::Serialize))]
    #[derive(Debug, Clone, Default)]
    pub struct CtrlTlsSnapshot {
        /// Mode: "insecure" | "system" | "pin" | "tofu"
        pub mode: String,
        /// SNI used for TLS handshake (if applicable)
        pub sni: Option<String>,
        /// Pinned certificate SHA-256 (hex, DER) for pin or TOFU
        pub pin_sha256_hex: Option<String>,
        /// Length of extra CA PEM bytes provided (if any)
        pub ca_pem_len: Option<usize>,
    }

    /// Client core that can run headless; UI layers subscribe to events.
    #[derive(Clone)]
    pub struct Client {
        state: Arc<RwLock<State>>,
        tx_evt: broadcast::Sender<ClientEvent>,
        cmd_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    }

    impl Default for Client {
        fn default() -> Self { Self::new() }
    }

    impl Client {
        /// Create a new client instance.
        #[must_use]
        pub fn new() -> Self {
            let (tx_evt, _rx) = broadcast::channel(64);
            Self {
                state: Arc::new(RwLock::new(State::default())),
                tx_evt,
                cmd_task: Arc::new(RwLock::new(None)),
            }
        }

        /// Subscribe to client events (UI calls this to update state).
        pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> { self.tx_evt.subscribe() }

        /// Returns whether the client is currently running.
        pub async fn is_running(&self) -> bool {
            self.state.read().await.running
        }

        /// Snapshot of control channel status.
        pub async fn ctrl_status(&self) -> CtrlSnapshot {
            let s = self.state.read().await;
            CtrlSnapshot {
                connected: s.ctrl_connected,
                last_disconnect: s.last_disconnect.clone(),
                reconnects: s.reconnects,
                last_connected_at: s.last_connected_at,
                last_disconnect_at: s.last_disconnect_at,
                last_error: s.last_error.clone(),
                last_attempt_at: s.last_attempt_at,
                attempts: s.attempts,
                ping_interval_secs: s.ctrl_ping_secs,
                backoff_base_ms: s.ctrl_backoff_base_ms,
                backoff_max_ms: s.ctrl_backoff_max_ms,
                tls: s.tls.clone(),
            }
        }

        /// Start the agent loop (idempotent). Emits `ClientEvent::Started` on success.
        pub async fn start(&self) -> Result<(), ClientError> {
            let mut s = self.state.write().await;
            if s.running { return Err(ClientError::AlreadyRunning); }
            s.running = true;
            let tx = self.tx_evt.clone();
            let handle = tokio::spawn(async move {
                let _ = tx.send(ClientEvent::Started);
                let mut ticker = tokio::time::interval(Duration::from_secs(5));
                loop {
                    ticker.tick().await;
                    // future: background work (keepalive, registration, policy checks)
                }
            });
            *self.cmd_task.write().await = Some(handle);
            Ok(())
        }

        /// Stop the agent loop. Emits `ClientEvent::Stopped`.
        pub async fn stop(&self) -> Result<(), ClientError> {
            let mut s = self.state.write().await;
            if !s.running { return Err(ClientError::NotRunning); }
            s.running = false;
            if let Some(h) = self.cmd_task.write().await.take() { h.abort(); }
            let _ = self.tx_evt.send(ClientEvent::Stopped);
            Ok(())
        }

        /// Start the agent and attach a QUIC control channel to receive commands and liveness.
        /// - `server`: QUIC server address
        /// - `token`: bearer for handshake auth
        pub async fn start_with_ctrl(&self, server: std::net::SocketAddr, token: String) -> Result<(), ClientError> {
            // Delegate to tuned variant with defaults
            self.start_with_ctrl_tuned(
                server,
                token,
                CtrlClientConfig::default().ping_interval_secs,
                CtrlClientConfig::default().backoff_base_ms,
                CtrlClientConfig::default().backoff_max_ms,
            )
            .await
        }

        /// Start the agent and attach a QUIC control channel with explicit tunables.
        /// - `server`: QUIC server address
        /// - `token`: bearer for handshake auth
        /// - `ping_interval_secs`: heartbeat ping interval in seconds
        /// - `backoff_base_ms`: reconnect backoff base in milliseconds
        /// - `backoff_max_ms`: reconnect backoff cap in milliseconds
        pub async fn start_with_ctrl_tuned(
            &self,
            server: std::net::SocketAddr,
            token: String,
            ping_interval_secs: u64,
            backoff_base_ms: u64,
            backoff_max_ms: u64,
        ) -> Result<(), ClientError> {
            // Start local loop if not running
            let _ = self.start().await;
            let tx_evt = self.tx_evt.clone();
            let me = self.clone();
            let handle = tokio::spawn(async move {
                // Connect control channel
                let cfg = CtrlClientConfig { ping_interval_secs, backoff_base_ms, backoff_max_ms };
                match quic_ctrl::run_ctrl_client(server, token, cfg).await {
                    Ok((_join, mut rx)) => {
                        let _ = tx_evt.send(ClientEvent::SessionIncoming { peer_id: server.to_string() });
                        while let Some(ev) = rx.recv().await {
                            match ev {
                                CtrlEvent::Liveness => {
                                    // Could update a last-seen timestamp; emit no-op for now
                                }
                                CtrlEvent::Connected => {
                                    let _ = tx_evt.send(ClientEvent::CtrlConnected);
                                    let mut st = me.state.write().await;
                                    st.ctrl_connected = true;
                                    st.last_disconnect = None;
                                    st.last_error = None;
                                    st.last_connected_at = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                                    st.ctrl_ping_secs = Some(cfg.ping_interval_secs);
                                    st.ctrl_backoff_base_ms = Some(cfg.backoff_base_ms);
                                    st.ctrl_backoff_max_ms = Some(cfg.backoff_max_ms);
                                }
                                CtrlEvent::Disconnected(reason) => {
                                    let _ = tx_evt.send(ClientEvent::CtrlDisconnected { reason: reason.clone() });
                                    let mut st = me.state.write().await;
                                    st.ctrl_connected = false;
                                    st.last_disconnect = Some(reason.clone());
                                    st.reconnects = st.reconnects.saturating_add(1);
                                    st.last_disconnect_at = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                                    st.last_error = Some(reason);
                                    st.attempts = st.attempts.saturating_add(1);
                                    st.last_attempt_at = st.last_disconnect_at;
                                }
                                CtrlEvent::Command(Cmd::Start) => {
                                    let _ = me.start().await;
                                }
                                CtrlEvent::Command(Cmd::Stop) => {
                                    let _ = me.stop().await;
                                }
                                CtrlEvent::AuthRenewed => {
                                    let _ = tx_evt.send(ClientEvent::CtrlAuthRenewed);
                                }
                                CtrlEvent::Error(err) => {
                                    let mut st = me.state.write().await;
                                    st.last_error = Some(err);
                                }
                            }
                        }
                    }
                    Err(_e) => {
                        let _ = tx_evt.send(ClientEvent::ConsentRequired { reason: "ctrl_connect_failed" });
                    }
                }
            });
            *self.cmd_task.write().await = Some(handle);
            Ok(())
        }

        /// Start the agent and attach a QUIC control channel over TLS with explicit tunables and trust mode.
        /// This updates the internal ctrl status and TLS trust snapshot for reporting via /status and /ctrl/config.
        pub async fn start_with_ctrl_tls_tuned(
            &self,
            server: std::net::SocketAddr,
            token: String,
            ping_interval_secs: u64,
            backoff_base_ms: u64,
            backoff_max_ms: u64,
            mut tls: transport::quic_ctrl::TlsMode,
            cached_pin: Option<String>,
            cached_ca: Option<Vec<u8>>,
        ) -> Result<(), ClientError> {
            // Start local loop if not running
            let _ = self.start().await;
            // Pre-populate TLS snapshot based on provided mode and caches
            {
                let mut st = self.state.write().await;
                let mut snap = CtrlTlsSnapshot::default();
                match &tls {
                    transport::quic_ctrl::TlsMode::InsecureNoVerify => {
                        snap.mode = "insecure".to_string();
                    }
                    transport::quic_ctrl::TlsMode::SystemRoots { sni, ca_pem } => {
                        snap.mode = "system".to_string();
                        snap.sni = Some(sni.clone());
                        snap.ca_pem_len = ca_pem.as_ref().map(|v| v.len());
                    }
                    transport::quic_ctrl::TlsMode::PinSha256 { sni, der_sha256_hex } => {
                        snap.mode = "pin".to_string();
                        snap.sni = Some(sni.clone());
                        snap.pin_sha256_hex = Some(der_sha256_hex.clone());
                    }
                    transport::quic_ctrl::TlsMode::Tofu { sni, .. } => {
                        snap.mode = "tofu".to_string();
                        snap.sni = Some(sni.clone());
                        snap.pin_sha256_hex = cached_pin.clone();
                    }
                }
                st.ctrl_ping_secs = Some(ping_interval_secs);
                st.ctrl_backoff_base_ms = Some(backoff_base_ms);
                st.ctrl_backoff_max_ms = Some(backoff_max_ms);
                st.tls = Some(snap);
            }
            // If TOFU, wrap on_first to also record the learned pin in state
            if let transport::quic_ctrl::TlsMode::Tofu { sni, on_first } = tls {
                let me = self.clone();
                let wrapped = std::sync::Arc::new(move |pin: String| {
                    // Best-effort, non-blocking update of TLS snapshot
                    if let Ok(mut guard) = me.state.try_write() {
                        if let Some(ref mut ts) = guard.tls {
                            ts.pin_sha256_hex = Some(pin.clone());
                        }
                    }
                    // Chain to provided callback
                    (on_first)(pin);
                });
                tls = transport::quic_ctrl::TlsMode::Tofu { sni, on_first: wrapped };
            }
            let tx_evt = self.tx_evt.clone();
            let me = self.clone();
            let handle = tokio::spawn(async move {
                let cfg = quic_ctrl::CtrlClientConfig { ping_interval_secs, backoff_base_ms, backoff_max_ms };
                match quic_ctrl::run_ctrl_client_with_tls(server, token, cfg, tls, cached_pin, cached_ca).await {
                    Ok((_join, mut rx)) => {
                        let _ = tx_evt.send(ClientEvent::SessionIncoming { peer_id: server.to_string() });
                        while let Some(ev) = rx.recv().await {
                            match ev {
                                quic_ctrl::CtrlEvent::Liveness => {}
                                quic_ctrl::CtrlEvent::Connected => {
                                    let _ = tx_evt.send(ClientEvent::CtrlConnected);
                                    let mut st = me.state.write().await;
                                    st.ctrl_connected = true;
                                    st.last_disconnect = None;
                                    st.last_error = None;
                                    st.last_connected_at = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                                }
                                quic_ctrl::CtrlEvent::Disconnected(reason) => {
                                    let _ = tx_evt.send(ClientEvent::CtrlDisconnected { reason: reason.clone() });
                                    let mut st = me.state.write().await;
                                    st.ctrl_connected = false;
                                    st.last_disconnect = Some(reason.clone());
                                    st.reconnects = st.reconnects.saturating_add(1);
                                    st.last_disconnect_at = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                                    st.last_error = Some(reason);
                                    st.attempts = st.attempts.saturating_add(1);
                                    st.last_attempt_at = st.last_disconnect_at;
                                }
                                quic_ctrl::CtrlEvent::Command(quic_ctrl::Cmd::Start) => {
                                    let _ = me.start().await;
                                }
                                quic_ctrl::CtrlEvent::Command(quic_ctrl::Cmd::Stop) => {
                                    let _ = me.stop().await;
                                }
                                quic_ctrl::CtrlEvent::AuthRenewed => {
                                    let _ = tx_evt.send(ClientEvent::CtrlAuthRenewed);
                                }
                                quic_ctrl::CtrlEvent::Error(err) => {
                                    let mut st = me.state.write().await;
                                    st.last_error = Some(err);
                                }
                            }
                        }
                    }
                    Err(_e) => {
                        let _ = tx_evt.send(ClientEvent::ConsentRequired { reason: "ctrl_tls_connect_failed" });
                    }
                }
            });
            *self.cmd_task.write().await = Some(handle);
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn start_stop_produces_events() {
            let c = Client::new();
            let mut rx = c.subscribe();
            c.start().await.unwrap();
            // Receive Started
            let _ = rx.recv().await.unwrap();
            c.stop().await.unwrap();
            let _ = rx.recv().await.unwrap();
        }
    }
}

#[cfg(feature = "http-ui")]
pub mod http_ui {
    use super::core;
    #[cfg(feature = "macos-capture")]
    use super::capture;
    use bytes::Bytes;
    use http_body_util::{Full, BodyExt, combinators::BoxBody, StreamBody};
    use hyper::body::Frame;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Method, Request, Response, StatusCode};
    use serde::Serialize;
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use tokio::net::TcpListener;
    use tokio::sync::Notify;
    use tokio::task::JoinHandle;
    use anyhow::Result;
    use hyper_util::rt::TokioIo;
    use hyper::header::AUTHORIZATION;
    use std::path::{Path, PathBuf};
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_stream::StreamExt;
    use std::time::Instant;
    
    use std::str::FromStr;

    use serde::Deserialize;
    use tokio::sync::Mutex;
    #[cfg(all(target_os = "macos", feature = "macos-input"))]
    use std::ffi::c_void;
    #[cfg(all(target_os = "macos", feature = "macos-input"))]
    use core_graphics::display::{CGMainDisplayID, CGDisplayPixelsHigh, CGDisplayPixelsWide};
    #[cfg(all(target_os = "macos", feature = "macos-input"))]
    use core_graphics::geometry::CGRect as CGR;

    #[derive(Debug, Serialize)]
    struct StatusPayload {
        running: bool,
        ctrl: Option<serde_json::Value>,
        rates: RateStatus,
        consent_allowed: bool,
    }

    #[derive(Debug, Serialize)]
    struct CtrlConfigPayload {
        ctrl: Option<serde_json::Value>,
    }

    #[derive(Clone, Debug)]
    pub struct StreamConfig {
        pub default_width: u32,
        pub default_height: u32,
        pub default_fps: u32,
        pub default_quality: u8,
    }

    #[derive(Debug)]
    struct RateLimiter {
        capacity: f64,
        refill_per_sec: f64,
        inner: Mutex<(f64, Instant)>,
        calls: Mutex<u64>,
        denied: Mutex<u64>,
    }

    impl RateLimiter {
        fn new(capacity: f64, refill_per_sec: f64) -> Self {
            Self { capacity, refill_per_sec, inner: Mutex::new((capacity, Instant::now())), calls: Mutex::new(0), denied: Mutex::new(0) }
        }
        async fn allow(&self, tokens: f64) -> bool {
            {
                let mut c = self.calls.lock().await; *c += 1;
            }
            let mut guard = self.inner.lock().await;
            let now = Instant::now();
            let elapsed = now.duration_since(guard.1).as_secs_f64();
            guard.0 = (guard.0 + elapsed * self.refill_per_sec).min(self.capacity);
            guard.1 = now;
            if guard.0 >= tokens { guard.0 -= tokens; true } else { let mut d = self.denied.lock().await; *d += 1; false }
        }
    }

    #[derive(Debug, Serialize, Default, Clone, Copy)]
    struct RateStatus { post_calls: u64, post_denied: u64, stream_calls: u64, stream_denied: u64 }

    async fn handle(
        req: Request<Incoming>,
        client: core::Client,
        token: Option<Arc<String>>,
        static_dir: Option<Arc<PathBuf>>,
        stream_cfg: Arc<StreamConfig>,
        clip_cache: Arc<ClipCache>,
        disp_state: Arc<DisplayState>,
        allowed_origins: Arc<Option<Vec<String>>>,
        rl_post: Arc<RateLimiter>,
        rl_stream: Arc<RateLimiter>,
        consent_allowed: Arc<Mutex<bool>>,
    ) -> Result<Response<BoxBody<Bytes, Infallible>>, Infallible> {
        let path = req.uri().path();
        let origin_hdr = req.headers().get(hyper::header::ORIGIN).and_then(|v| v.to_str().ok()).map(|s| s.to_string());
        let is_origin_allowed = |origin: &str, allow: &Option<Vec<String>>| -> bool {
            match allow {
                Some(list) => list.iter().any(|o| o == origin || o == "*"),
                None => false,
            }
        };
        let authorized = |req: &Request<Incoming>| -> bool {
            match &token {
                None => true,
                Some(tok) => {
                    // Check Authorization header first
                    if let Some(hv) = req.headers().get(AUTHORIZATION) {
                        if let Ok(val) = hv.to_str() {
                            if val == format!("Bearer {}", tok.as_str()) {
                                return true;
                            }
                        }
                    }
                    // Fallback: token in query string (?token=...)
                    if let Some(qs) = req.uri().query() {
                        for kv in qs.split('&') {
                            if let Some((k, v)) = kv.split_once('=') {
                                if k == "token" && v == tok.as_str() { return true; }
                            }
                        }
                    }
                    false
                }
            }
        };
        // Static file serving (if configured) takes precedence for GET /
        let mut response = match (req.method(), path) {
            (&Method::OPTIONS, _) => {
                Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Full::new(Bytes::new()).boxed())
                    .unwrap()
            },
            (&Method::GET, "/") => {
                if let Some(dir) = &static_dir {
                    match serve_static(dir, "index.html").await {
                        Ok((body, content_type)) => Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", content_type)
                            .header(hyper::header::CACHE_CONTROL, "no-store")
                            .body(Full::new(body).boxed())
                            .unwrap(),
                        Err(StatusCode::NOT_FOUND) => Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .body(Full::new(Bytes::new()).boxed())
                            .unwrap(),
                        Err(_) => Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(Full::new(Bytes::new()).boxed())
                            .unwrap(),
                    }
                } else {
                    const HTML: &str = r#"<!doctype html><html><head><title>QuicView Client</title></head><body><h1>QuicView Client</h1></body></html>"#;
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/html; charset=utf-8")
                        .header(hyper::header::CACHE_CONTROL, "no-store")
                        .body(Full::new(Bytes::from_static(HTML.as_bytes())).boxed())
                        .unwrap()
                }
            },
            (&Method::GET, "/favicon.ico") => {
                Response::builder()
                    .status(StatusCode::NO_CONTENT)
                    .body(Full::new(Bytes::new()).boxed())
                    .unwrap()
            },
            (&Method::GET, "/status") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let running = client.is_running().await;
                    let ctrl: Option<serde_json::Value> = Some(serde_json::to_value(client.ctrl_status().await).unwrap());
                    let rates = RateStatus {
                        post_calls: *rl_post.calls.lock().await,
                        post_denied: *rl_post.denied.lock().await,
                        stream_calls: *rl_stream.calls.lock().await,
                        stream_denied: *rl_stream.denied.lock().await,
                    };
                    let consent_allowed = *consent_allowed.lock().await;
                    let body = serde_json::to_vec(&StatusPayload { running, ctrl, rates, consent_allowed }).unwrap();
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header(hyper::header::CACHE_CONTROL, "no-store")
                        .body(Full::new(Bytes::from(body)).boxed())
                        .unwrap()
                }
            },
            (&Method::POST, "/start") => {
                if !rl_post.allow(1.0).await {
                    Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let _ = client.start().await;
                    Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap()
                }
            },
            (&Method::POST, "/stop") => {
                if !rl_post.allow(1.0).await {
                    Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let _ = client.stop().await;
                    Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap()
                }
            },
            (&Method::GET, "/clipboard") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    match get_clipboard_text(&clip_cache).await {
                        Ok(text) => {
                            let body = serde_json::to_vec(&serde_json::json!({ "text": text })).unwrap();
                            Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", "application/json")
                                .header(hyper::header::CACHE_CONTROL, "no-store")
                                .body(Full::new(Bytes::from(body)).boxed())
                                .unwrap()
                        }
                        Err(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap(),
                    }
                }
            },
            (&Method::POST, "/clipboard") => {
                if !rl_post.allow(1.0).await {
                    Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let whole = req.collect().await;
                    match whole {
                        Ok(b) => {
                            let body = b.to_bytes();
                            let text_opt = serde_json::from_slice::<serde_json::Value>(&body)
                                .ok()
                                .and_then(|j| j.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()));
                            if let Some(text) = text_opt {
                                let ok = set_clipboard_text(&clip_cache, text).await.is_ok();
                                if ok { Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap() }
                                else { Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap() }
                            } else {
                                Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap()
                            }
                        }
                        Err(_) => Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap(),
                    }
                }
            },
            (&Method::GET, "/consent") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let allowed = *consent_allowed.lock().await;
                    let body = serde_json::to_vec(&serde_json::json!({ "allowed": allowed })).unwrap();
                    Response::builder().status(StatusCode::OK).header("content-type", "application/json").body(Full::new(Bytes::from(body)).boxed()).unwrap()
                }
            },
            (&Method::POST, "/consent/allow") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    *consent_allowed.lock().await = true;
                    Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap()
                }
            },
            (&Method::POST, "/consent/deny") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    *consent_allowed.lock().await = false;
                    Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap()
                }
            },
            (&Method::GET, p) if static_dir.is_some() => {
                // Try to serve static file for any GET path
                let rel = p.trim_start_matches('/');
                if rel.is_empty() || rel.contains("..") {
                    Response::builder().status(StatusCode::NOT_FOUND).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let dir = static_dir.as_ref().unwrap();
                    match serve_static(dir, rel).await {
                        Ok((body, content_type)) => {
                            let cache = if rel.ends_with(".js") || rel.ends_with(".wasm") { "public, max-age=604800, immutable" } else { "no-store" };
                            Response::builder()
                                .status(StatusCode::OK)
                                .header("content-type", content_type)
                                .header(hyper::header::CACHE_CONTROL, cache)
                                .body(Full::new(body).boxed())
                                .unwrap()
                        }
                        Err(StatusCode::NOT_FOUND) => Response::builder().status(StatusCode::NOT_FOUND).body(Full::new(Bytes::new()).boxed()).unwrap(),
                        Err(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap(),
                    }
                }
            },
            _ => Response::builder().status(StatusCode::NOT_FOUND).body(Full::new(Bytes::new()).boxed()).unwrap(),
        };
        // Attach CORS based on allowlist and add security headers
        let headers = response.headers_mut();
        headers.insert(hyper::header::REFERRER_POLICY, "no-referrer".parse().unwrap());
        headers.insert(hyper::header::HeaderName::from_static("x-content-type-options"), "nosniff".parse().unwrap());
        headers.insert(
            hyper::header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: http://127.0.0.1:* http://localhost:*; connect-src 'self' http://127.0.0.1:* http://localhost:*; frame-ancestors 'none'; object-src 'none'; base-uri 'self'"
                .parse()
                .unwrap(),
        );
        headers.insert(hyper::header::HeaderName::from_static("x-frame-options"), "DENY".parse().unwrap());
        headers.insert(hyper::header::HeaderName::from_static("cross-origin-opener-policy"), "same-origin".parse().unwrap());
        headers.insert(hyper::header::HeaderName::from_static("permissions-policy"), "camera=(), microphone=(), geolocation=()".parse().unwrap());
        // CORS allowlist
        if let Some(origin) = origin_hdr.as_deref() {
            if is_origin_allowed(origin, &allowed_origins) {
                headers.insert(hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN, origin.parse().unwrap());
                headers.insert(hyper::header::VARY, "Origin".parse().unwrap());
                headers.insert(hyper::header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, OPTIONS".parse().unwrap());
                headers.insert(hyper::header::ACCESS_CONTROL_ALLOW_HEADERS, "authorization, content-type".parse().unwrap());
            }
        }
        Ok(response)
    }

    async fn serve_static(dir: &Arc<PathBuf>, rel: &str) -> Result<(Bytes, &'static str), StatusCode> {
        let path = Path::new(&**dir).join(rel);
        let ct = match Path::new(rel).extension().and_then(|s| s.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8",
            "js" => "text/javascript; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "json" => "application/json",
            "wasm" => "application/wasm",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "svg" => "image/svg+xml",
            "ico" => "image/x-icon",
            _ => "application/octet-stream",
        };
        match tokio::fs::read(&path).await {
            Ok(data) => Ok((Bytes::from(data), ct)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(StatusCode::NOT_FOUND),
            Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    pub struct HttpHandle {
        pub addr: SocketAddr,
        join: JoinHandle<()>,
        shutdown: Arc<Notify>,
    }

    impl HttpHandle {
        pub async fn shutdown(self) {
            self.shutdown.notify_waiters();
            let _ = self.join.await;
        }
    }

    /// Start a tiny HTTP control server.
    pub async fn serve(
        bind: SocketAddr,
        client: core::Client,
        token: Option<String>,
        static_dir: Option<PathBuf>,
        stream_defaults: Option<StreamConfig>,
        allowed_origins: Option<Vec<String>>,
        rl_post_cfg: Option<(f64, f64)>,
        rl_stream_cfg: Option<(f64, f64)>,
        consent_default: Option<bool>,
    ) -> Result<HttpHandle> {
        let listener = TcpListener::bind(bind).await?;
        let local = listener.local_addr()?;
        let shutdown = Arc::new(Notify::new());
        let shutdown_task = shutdown.clone();
        let svc_client = client.clone();
        let token = token.map(Arc::new);
        let static_dir = static_dir.map(Arc::new);
        let stream_cfg = Arc::new(stream_defaults.unwrap_or(StreamConfig {
            default_width: 320,
            default_height: 180,
            default_fps: 5,
            default_quality: 70,
        }));
        let clip_cache = Arc::new(ClipCache::default());
        let disp_state = Arc::new(DisplayState::default());
        let allow = Arc::new(allowed_origins);
        let (pb, pr) = rl_post_cfg.unwrap_or((30.0, 15.0));
        let (sb, sr) = rl_stream_cfg.unwrap_or((5.0, 2.0));
        let rl_post = Arc::new(RateLimiter::new(pb, pr));
        let rl_stream = Arc::new(RateLimiter::new(sb, sr));
        let consent_allowed = Arc::new(Mutex::new(consent_default.unwrap_or(true)));
        let join = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = shutdown_task.notified() => break,
                    accept_result = listener.accept() => {
                        if let Ok((stream, _peer)) = accept_result {
                            let io = TokioIo::new(stream);
                            let c = svc_client.clone();
                            let tok = token.clone();
                            let static_dir2 = static_dir.clone();
                            let stream_cfg2 = stream_cfg.clone();
                            let clip2 = clip_cache.clone();
                            let disp2 = disp_state.clone();
                            let allow2 = allow.clone();
                            let rl_post_conn = rl_post.clone();
                            let rl_stream_conn = rl_stream.clone();
                            let consent_conn = consent_allowed.clone();
                            let svc = service_fn(move |req| {
                                let c2 = c.clone();
                                let tok2 = tok.clone();
                                let sdir = static_dir2.clone();
                                let sc = stream_cfg2.clone();
                                let cc = clip2.clone();
                                let ds = disp2.clone();
                                let allow3 = allow2.clone();
                                let rl_p = rl_post_conn.clone();
                                let rl_s = rl_stream_conn.clone();
                                let cons = consent_conn.clone();
                                handle(req, c2, tok2, sdir, sc, cc, ds, allow3, rl_p, rl_s, cons)
                            });
                            tokio::spawn(async move {
                                let _ = http1::Builder::new().serve_connection(io, svc).await;
                            });
                        } else {
                            break;
                        }
                    }
                }
            }
        });
        Ok(HttpHandle { addr: local, join, shutdown })
    }

    #[derive(Default)]
    struct DisplayState { selected: Mutex<Option<u32>> }

    #[derive(Default)]
    struct ClipCache {
        inner: Mutex<String>,
    }

    async fn get_clipboard_text(cache: &Arc<ClipCache>) -> Result<String, ()> {
        #[cfg(feature = "clipboard")]
        {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                if let Ok(s) = cb.get_text() {
                    return Ok(s);
                }
            }
        }
        Ok(cache.inner.lock().await.clone())
    }

    async fn set_clipboard_text(cache: &Arc<ClipCache>, text: String) -> Result<(), ()> {
        #[cfg(feature = "clipboard")]
        {
            if let Ok(mut cb) = arboard::Clipboard::new() {
                let _ = cb.set_text(text.clone());
            }
        }
        *cache.inner.lock().await = text;
        Ok(())
    }
}
