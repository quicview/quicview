//! QuicView client crate
//! - Headless client core (`core` module) for UI-agnostic operation
//! - QUIC client for direct server connection (`quic_client` module)
//! - Legacy `ClientLauncher` stub retained for backward compatibility

use bridge::{BridgeError, ClientLauncher};
use config::QuicViewConfig;

mod capture;

/// QUIC client for connecting to a QuicView server.
/// Provides screen frame reception, input event sending, and clipboard sync.
#[cfg(feature = "quic-data")]
pub mod quic_client;

#[cfg(feature = "quic-data")]
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
    #[cfg(feature = "quic-ctrl")]
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
    // TLS trust state (if QUIC/TLS control channel is configured)
    #[cfg(feature = "quic-ctrl")]
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
        /// TLS trust snapshot (present when using QUIC/TLS control channel)
        #[cfg(feature = "quic-ctrl")]
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
                #[cfg(feature = "quic-ctrl")]
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
        #[cfg(feature = "quic-ctrl")]
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
        #[cfg(feature = "quic-ctrl")]
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
        #[cfg(feature = "quic-ctrl")]
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
                                                            const HTML: &str = r#"<!doctype html>
                    <html>
                        <head>
                            <meta charset='utf-8'/>
                            <meta name='viewport' content='width=device-width, initial-scale=1'>
                            <title>QuicView Client</title>
                            <link rel='icon' href='/favicon.ico'>
                            <link rel='stylesheet' href='/_ui/app.css'>
                        </head>
                        <body>
                            <header class='topbar'>
                                <h1>QuicView Client</h1>
                                <div class='spacer'></div>
                                <span id='badge-running' class='badge'>stopped</span>
                            </header>
                            <main class='container'>
                                <section class='card'>
                                    <h2>Controls</h2>
                                    <div class='row'>
                                        <button id='start' class='btn primary'>Start</button>
                                        <button id='stop' class='btn'>Stop</button>
                                        <span id='status' class='muted'>Status: ...</span>
                                    </div>
                                    <div class='row'>
                                        <button id='allow' class='btn success'>Allow Control</button>
                                        <button id='deny' class='btn danger'>Deny Control</button>
                                        <span id='consent' class='muted'>Consent: ...</span>
                                    </div>
                                </section>

                                <section class='card'>
                                    <h2>Displays</h2>
                                    <div class='row'>
                                        <select id='display-select'></select>
                                        <button id='display-apply' class='btn'>Select</button>
                                        <span id='display-info' class='muted'></span>
                                    </div>
                                </section>

                                <section class='card'>
                                    <h2>Stream (MJPEG)</h2>
                                    <div class='row'>
                                        <label>W <input id='w' type='number' min='64' max='4096' value='640'></label>
                                        <label>H <input id='h' type='number' min='36' max='2160' value='360'></label>
                                        <label>FPS <input id='fps' type='number' min='1' max='60' value='10'></label>
                                        <label>Q <input id='q' type='number' min='30' max='95' value='70'></label>
                                        <button id='stream-start' class='btn'>Start Stream</button>
                                        <button id='stream-stop' class='btn'>Stop Stream</button>
                                        <label><input id='fit' type='checkbox' checked> Fit</label>
                                        <label>Zoom <input id='zoom' type='range' min='50' max='200' value='100'></label>
                                        <label><input id='auto-reconnect' type='checkbox' checked> Auto-reconnect</label>
                                        <span id='stream-stats' class='muted'></span>
                                    </div>
                                    <div class='streambox'>
                                        <img id='stream' alt='stream preview'>
                                    </div>
                                </section>

                                <section class='card'>
                                    <h2>Clipboard</h2>
                                    <div class='row'>
                                        <button id='clip-load' class='btn'>Load</button>
                                        <button id='clip-save' class='btn'>Save</button>
                                    </div>
                                    <textarea id='clip' rows='5' placeholder='Clipboard text...'></textarea>
                                </section>

                                <section class='card'>
                                    <h2>Control Channel</h2>
                                    <pre id='ctrl' class='box'></pre>
                                </section>

                                <section class='card'>
                                    <h2>Rates</h2>
                                    <pre id='rates' class='box'></pre>
                                </section>
                                    </main>
                                    <div id='toasts' class='toasts'></div>
                                    <footer class='footer muted'>
                                Tip: supply a token via URL like <code>#token=dev-token</code> to authorize actions.
                            </footer>
                            <script src='/_ui/app.js'></script>
                        </body>
                    </html>"#;
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/html; charset=utf-8")
                        .header(hyper::header::CACHE_CONTROL, "no-store")
                        .body(Full::new(Bytes::from_static(HTML.as_bytes())).boxed())
                        .unwrap()
                }
            },
                        // Minimal assets for the inline UI (avoid inline CSS/JS to satisfy CSP)
                        (&Method::GET, "/_ui/app.css") => {
                            if let Some(dir) = &static_dir {
                                match serve_static(dir, "app.css").await {
                                    Ok((body, content_type)) => Response::builder()
                                        .status(StatusCode::OK)
                                        .header("content-type", content_type)
                                        .header(hyper::header::CACHE_CONTROL, "no-store")
                                        .body(Full::new(body).boxed())
                                        .unwrap(),
                                    Err(StatusCode::NOT_FOUND) => {
                                        const CSS: &str = r#":root{--bg:#0b0c0e;--panel:#15171a;--muted:#8b949e;--text:#e6edf3;--primary:#2a74ff;--ok:#2ea043;--danger:#f85149;--border:#30363d}
                        *{box-sizing:border-box}
                        html,body{height:100%}
                        body{margin:0;background:var(--bg);color:var(--text);font:14px/1.4 -apple-system,BlinkMacSystemFont,Segoe UI,Roboto,Ubuntu,"Helvetica Neue",Arial}
                        .topbar{display:flex;align-items:center;gap:12px;padding:12px 16px;background:#0f1115;border-bottom:1px solid var(--border)}
                        .topbar h1{font-size:18px;margin:0}
                        .container{padding:16px;display:grid;grid-template-columns:repeat(auto-fit,minmax(320px,1fr));gap:16px}
                        .card{background:var(--panel);border:1px solid var(--border);border-radius:8px;padding:12px}
                        .row{display:flex;gap:8px;align-items:center;flex-wrap:wrap;margin:8px 0}
                        .btn{background:#24292f;color:var(--text);border:1px solid var(--border);border-radius:6px;padding:6px 10px;cursor:pointer}
                        .btn:hover{filter:brightness(1.1)}
                        .btn.primary{background:var(--primary);border-color:#1f5fe0}
                        .btn.success{background:var(--ok);border-color:#248a37}
                        .btn.danger{background:var(--danger);border-color:#dd3b38}
                        .muted{color:var(--muted)}
                        .badge{padding:2px 8px;border-radius:999px;border:1px solid var(--border);background:#1b1f24}
                        .badge.ok{background:#122814;border-color:#1f6f2e}
                        .badge.err{background:#2a1214;border-color:#a52824}
                        .box{background:#0f1115;border:1px solid var(--border);border-radius:6px;padding:8px;max-width:100%;overflow:auto}
                        .streambox{background:#0f1115;border:1px dashed var(--border);border-radius:6px;min-height:120px;display:flex;align-items:center;justify-content:center;overflow:auto;position:relative}
                        img#stream{max-width:100%;height:auto;transform-origin:0 0}
                        input[type=range]{accent-color:var(--primary)}
                        textarea#clip{width:100%;background:#0f1115;color:var(--text);border:1px solid var(--border);border-radius:6px;padding:8px}
                        .footer{padding:12px 16px;border-top:1px solid var(--border)}
                        .spacer{flex:1}
                        code{background:#0f1115;border:1px solid var(--border);padding:2px 4px;border-radius:4px}
                        label{display:inline-flex;align-items:center;gap:6px}
                        input[type=number]{width:72px;background:#0f1115;color:var(--text);border:1px solid var(--border);border-radius:6px;padding:4px}
                        select{background:#0f1115;color:var(--text);border:1px solid var(--border);border-radius:6px;padding:4px}
                        .toasts{position:fixed;right:16px;top:16px;display:flex;flex-direction:column;gap:8px;z-index:9999}
                        .toast{background:#1b1f24;border:1px solid var(--border);border-radius:6px;padding:8px 10px;min-width:200px;box-shadow:0 6px 18px rgba(0,0,0,0.3);transition:opacity .2s ease, transform .2s ease}
                        .toast.ok{border-color:#1f6f2e}
                        .toast.err{border-color:#a52824}
                        "#;
                                        Response::builder()
                                            .status(StatusCode::OK)
                                            .header("content-type", "text/css; charset=utf-8")
                                            .header(hyper::header::CACHE_CONTROL, "no-store")
                                            .body(Full::new(Bytes::from_static(CSS.as_bytes())).boxed())
                                            .unwrap()
                                    }
                                    Err(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap(),
                                }
                            } else {
                                const CSS: &str = "body{font-family:system-ui,Arial;margin:24px}button{margin-right:8px}.row{margin:8px 0}.box{background:#f6f8fa;padding:8px;border:1px solid #ddd;max-width:800px;overflow:auto}";
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header("content-type", "text/css; charset=utf-8")
                                    .header(hyper::header::CACHE_CONTROL, "no-store")
                                    .body(Full::new(Bytes::from_static(CSS.as_bytes())).boxed())
                                    .unwrap()
                            }
                        },
                        (&Method::GET, "/_ui/app.js") => {
                            if let Some(dir) = &static_dir {
                                match serve_static(dir, "app.js").await {
                                    Ok((body, content_type)) => Response::builder()
                                        .status(StatusCode::OK)
                                        .header("content-type", content_type)
                                        .header(hyper::header::CACHE_CONTROL, "no-store")
                                        .body(Full::new(body).boxed())
                                        .unwrap(),
                                    Err(StatusCode::NOT_FOUND) => {
                                                        const JS: &str = r#"(function(){
                        function getToken(){
                            try{
                                const h = window.location.hash || '';
                                const m = h.match(/#token=([^&]+)/);
                                if(m){ return decodeURIComponent(m[1]); }
                                const qs = new URLSearchParams(window.location.search);
                                if(qs.has('token')) return qs.get('token');
                            }catch(_){ }
                            return null;
                        }
                        const TOKEN = getToken();
                        function authHeaders(){ return TOKEN ? { 'Authorization': 'Bearer ' + TOKEN } : {}; }
                        function qsToken(){ return TOKEN ? ('token=' + encodeURIComponent(TOKEN)) : ''; }
                        async function jsonGet(path){ const r = await fetch(path, { headers: authHeaders() }); return r.json(); }
                        async function jsonPost(path, body){ const r = await fetch(path, { method:'POST', headers: Object.assign({'Content-Type':'application/json'}, authHeaders()), body: JSON.stringify(body||{}) }); return r; }

                        // Toasts
                        function toast(msg, cls){
                            const host = document.getElementById('toasts');
                            if(!host) return;
                            const el = document.createElement('div');
                            el.className = 'toast ' + (cls||'');
                            el.textContent = msg;
                            host.appendChild(el);
                            setTimeout(()=>{ el.style.opacity='0'; el.style.transform='translateY(-6px)'; }, 2300);
                            setTimeout(()=>{ host.removeChild(el); }, 3000);
                        }

                        function setBadge(running){
                            const b = document.getElementById('badge-running');
                            if(!b) return;
                            if(running){ b.textContent = 'running'; b.classList.add('ok'); b.classList.remove('err'); }
                            else { b.textContent = 'stopped'; b.classList.remove('ok'); b.classList.remove('err'); }
                        }

                        async function refresh(){
                            try{
                                const j = await jsonGet('/status');
                                document.getElementById('status').textContent = 'Status: ' + (j.running ? 'running' : 'stopped');
                                document.getElementById('consent').textContent = 'Consent: ' + (j.consent_allowed ? 'allowed' : 'denied');
                                setBadge(j.running);
                                const ctrlStr = JSON.stringify(j.ctrl || null, null, 2);
                                document.getElementById('ctrl').textContent = ctrlStr;
                                const ratesStr = JSON.stringify(j.rates || {}, null, 2);
                                document.getElementById('rates').textContent = ratesStr;
                            }catch(e){ /* ignore */ }
                        }

                        async function loadDisplays(){
                            try{
                                const d = await jsonGet('/displays');
                                const sel = document.getElementById('display-select');
                                sel.innerHTML='';
                                (d.displays||[]).forEach((x)=>{
                                    const opt = document.createElement('option');
                                    opt.value = String(x.id);
                                    opt.textContent = `${x.id} (${x.width}x${x.height})${x.is_main?' [main]':''}`;
                                    if(d.selected && x.id === d.selected){ opt.selected = true; }
                                    sel.appendChild(opt);
                                });
                                document.getElementById('display-info').textContent = d.selected ? ('Selected: '+d.selected) : 'No display selected';
                            }catch(e){ /* ignore */ }
                        }

                        function buildStreamUrl(){
                            const w = Number(document.getElementById('w').value||640);
                            const h = Number(document.getElementById('h').value||360);
                            const fps = Number(document.getElementById('fps').value||10);
                            const q = Number(document.getElementById('q').value||70);
                            const params = new URLSearchParams({ w:String(w), h:String(h), fps:String(fps), q:String(q) });
                            if(TOKEN) params.set('token', TOKEN);
                            return '/stream.mjpeg?' + params.toString();
                        }

                        // Stream controls
                        let reconnectTimer = null;
                        let backoffMs = 500;
                        let lastLoadTime = 0;
                        let frames = 0;
                        const statsEl = () => document.getElementById('stream-stats');
                        function updateStats(){
                            const now = performance.now();
                            const dt = (now - lastLoadTime) / 1000;
                            if(dt > 0 && frames>0) {
                                const fps = (frames/dt).toFixed(1);
                                const s = statsEl(); if(s) s.textContent = ` ${fps} fps`;
                                frames = 0; lastLoadTime = now;
                            }
                        }
                        setInterval(updateStats, 1000);

                        function applyFitZoom(){
                            const img = document.getElementById('stream');
                            const fit = document.getElementById('fit').checked;
                            const zoom = Number(document.getElementById('zoom').value||100)/100;
                            if(fit){
                                img.style.width = '100%';
                                img.style.height = 'auto';
                                img.style.transform = 'scale(1)';
                            } else {
                                img.style.width = 'auto';
                                img.style.height = 'auto';
                                img.style.transform = `scale(${zoom})`;
                            }
                        }

                        function startStream(){
                            const img = document.getElementById('stream');
                            const url = buildStreamUrl();
                            frames = 0; lastLoadTime = performance.now();
                            img.onerror = () => {
                                const auto = document.getElementById('auto-reconnect').checked;
                                if(auto){
                                    toast('Stream error, reconnecting...', 'err');
                                    clearTimeout(reconnectTimer);
                                    reconnectTimer = setTimeout(startStream, backoffMs);
                                    backoffMs = Math.min(backoffMs * 2, 10_000);
                                }
                            };
                            img.onload = () => { frames++; backoffMs = 500; };
                            img.src = url;
                            applyFitZoom();
                        }
                        function stopStream(){ const img = document.getElementById('stream'); img.src=''; clearTimeout(reconnectTimer); backoffMs = 500; const s = statsEl(); if(s) s.textContent=''; }

                        async function clipLoad(){ try { const j = await jsonGet('/clipboard'); document.getElementById('clip').value = j.text || ''; } catch(e){} }
                        async function clipSave(){ const text = document.getElementById('clip').value || ''; await jsonPost('/clipboard', { text }); }

                        async function init(){
                            document.getElementById('start').addEventListener('click', async ()=>{ await jsonPost('/start', {}); toast('Client start requested','ok'); await refresh(); });
                            document.getElementById('stop').addEventListener('click', async ()=>{ await jsonPost('/stop', {}); toast('Client stop requested','err'); await refresh(); });
                            document.getElementById('allow').addEventListener('click', async ()=>{ await jsonPost('/consent/allow', {}); toast('Control allowed','ok'); await refresh(); });
                            document.getElementById('deny').addEventListener('click', async ()=>{ await jsonPost('/consent/deny', {}); toast('Control denied','err'); await refresh(); });
                            document.getElementById('display-apply').addEventListener('click', async ()=>{ const id = Number(document.getElementById('display-select').value); if(!Number.isNaN(id)) { await jsonPost('/displays/select', { id }); await loadDisplays(); }});
                            document.getElementById('stream-start').addEventListener('click', startStream);
                            document.getElementById('stream-stop').addEventListener('click', stopStream);
                            document.getElementById('fit').addEventListener('change', applyFitZoom);
                            document.getElementById('zoom').addEventListener('input', applyFitZoom);
                            document.getElementById('clip-load').addEventListener('click', clipLoad);
                            document.getElementById('clip-save').addEventListener('click', clipSave);
                            await refresh();
                            await loadDisplays();
                            // SSE subscription with fallback to polling
                            try{
                                const url = '/events' + (TOKEN ? ('?token=' + encodeURIComponent(TOKEN)) : '');
                                const es = new EventSource(url);
                                let lastRunning = null;
                                es.addEventListener('status', (ev)=>{
                                    try{
                                        const j = JSON.parse(ev.data);
                                        document.getElementById('status').textContent = 'Status: ' + (j.running ? 'running' : 'stopped');
                                        document.getElementById('consent').textContent = 'Consent: ' + (j.consent_allowed ? 'allowed' : 'denied');
                                        setBadge(j.running);
                                        document.getElementById('ctrl').textContent = JSON.stringify(j.ctrl || null, null, 2);
                                        document.getElementById('rates').textContent = JSON.stringify(j.rates || {}, null, 2);
                                        if(lastRunning === null) { lastRunning = j.running; }
                                        else if(lastRunning !== j.running) { toast(j.running ? 'Client started' : 'Client stopped', j.running ? 'ok' : 'err'); lastRunning = j.running; }
                                    }catch(_){ }
                                });
                                es.onerror = ()=>{ es.close(); setInterval(refresh, 5000); };
                            }catch(_){ setInterval(refresh, 5000); }
                        }
                        document.addEventListener('DOMContentLoaded', init);
                        })();"#;
                                        Response::builder()
                                            .status(StatusCode::OK)
                                            .header("content-type", "text/javascript; charset=utf-8")
                                            .header(hyper::header::CACHE_CONTROL, "no-store")
                                            .body(Full::new(Bytes::from_static(JS.as_bytes())).boxed())
                                            .unwrap()
                                    }
                                    Err(_) => Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap(),
                                }
                            } else {
                                                                const JS: &str = r#"(function(){
                        function getToken(){
                            try{
                                const h = window.location.hash || '';
                                const m = h.match(/#token=([^&]+)/);
                                if(m){ return decodeURIComponent(m[1]); }
                                const qs = new URLSearchParams(window.location.search);
                                if(qs.has('token')) return qs.get('token');
                            }catch(_){ }
                            return null;
                        }
                        const TOKEN = getToken();
                        function authHeaders(){ return TOKEN ? { 'Authorization': 'Bearer ' + TOKEN } : {}; }
                        async function jsonGet(path){ const r = await fetch(path, { headers: authHeaders() }); return r.json(); }
                        async function jsonPost(path, body){ return fetch(path, { method:'POST', headers: Object.assign({'Content-Type':'application/json'}, authHeaders()), body: JSON.stringify(body||{}) }); }
                        function toast(msg, cls){ var host=document.getElementById('toasts'); if(!host) return; var el=document.createElement('div'); el.className='toast '+(cls||''); el.textContent=msg; host.appendChild(el); setTimeout(()=>{ el.style.opacity='0'; el.style.transform='translateY(-6px)'; },2300); setTimeout(()=>{ host.removeChild(el); },3000); }
                        function setBadge(running){ var b=document.getElementById('badge-running'); if(!b) return; if(running){ b.textContent='running'; b.classList.add('ok'); b.classList.remove('err'); } else { b.textContent='stopped'; b.classList.remove('ok'); b.classList.remove('err'); } }
                        async function refresh(){ try{ const j = await jsonGet('/status'); document.getElementById('status').textContent='Status: ' + (j.running?'running':'stopped'); document.getElementById('consent').textContent='Consent: ' + (j.consent_allowed?'allowed':'denied'); setBadge(j.running); document.getElementById('ctrl').textContent = JSON.stringify(j.ctrl||null, null, 2); var r = JSON.stringify(j.rates||{}, null, 2); var rs=document.getElementById('rates'); if(rs) rs.textContent=r; }catch(e){} }
                        async function init(){
                            document.getElementById('start').addEventListener('click', async ()=>{ await jsonPost('/start', {}); await refresh(); });
                            document.getElementById('stop').addEventListener('click', async ()=>{ await jsonPost('/stop', {}); await refresh(); });
                            document.getElementById('allow').addEventListener('click', async ()=>{ await jsonPost('/consent/allow', {}); await refresh(); });
                            document.getElementById('deny').addEventListener('click', async ()=>{ await jsonPost('/consent/deny', {}); await refresh(); });
                            await refresh();
                            try{
                                const url = '/events' + (TOKEN ? ('?token=' + encodeURIComponent(TOKEN)) : '');
                                const es = new EventSource(url);
                                let lastRunning = null;
                                es.addEventListener('status', (ev)=>{
                                    try{
                                        const j = JSON.parse(ev.data);
                                        document.getElementById('status').textContent='Status: ' + (j.running?'running':'stopped');
                                        document.getElementById('consent').textContent='Consent: ' + (j.consent_allowed?'allowed':'denied');
                                        setBadge(j.running);
                                        document.getElementById('ctrl').textContent = JSON.stringify(j.ctrl||null, null, 2);
                                        var r = JSON.stringify(j.rates||{}, null, 2); var rs=document.getElementById('rates'); if(rs) rs.textContent=r;
                                        if(lastRunning===null){ lastRunning = j.running; }
                                        else if(lastRunning !== j.running){ toast(j.running ? 'Client started' : 'Client stopped', j.running ? 'ok' : 'err'); lastRunning = j.running; }
                                    }catch(_){ }
                                });
                                es.onerror = ()=>{ es.close(); setInterval(refresh, 5000); };
                            }catch(_){ setInterval(refresh, 5000); }
                        }
                        document.addEventListener('DOMContentLoaded', init);
                        })();"#;
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .header("content-type", "text/javascript; charset=utf-8")
                                    .header(hyper::header::CACHE_CONTROL, "no-store")
                                    .body(Full::new(Bytes::from_static(JS.as_bytes())).boxed())
                                    .unwrap()
                            }
                        },
                        (&Method::GET, "/favicon.ico") => {
                                // Quiet 404s in dev: return empty icon
                                Response::builder()
                                        .status(StatusCode::NO_CONTENT)
                                        .body(Full::new(Bytes::new()).boxed())
                                        .unwrap()
                        },
            (&Method::GET, "/stream.mjpeg") => {
                if !rl_stream.allow(1.0).await {
                    Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !*consent_allowed.lock().await {
                    Response::builder().status(StatusCode::FORBIDDEN).body(Full::new(Bytes::from_static(b"{\"error\":\"consent_required\"}")).boxed()).unwrap()
                } else {
                    let boundary = "frame";
                    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(8);
                    let mut width = stream_cfg.default_width;
                    let mut height = stream_cfg.default_height;
                    let mut fps = stream_cfg.default_fps;
                    let mut quality = stream_cfg.default_quality;
                    if let Some(qs) = req.uri().query() {
                        for kv in qs.split('&') {
                            if let Some((k, v)) = kv.split_once('=') {
                                match k {
                                    "w" => if let Ok(n) = u32::from_str(v) { width = n; },
                                    "h" => if let Ok(n) = u32::from_str(v) { height = n; },
                                    "fps" => if let Ok(n) = u32::from_str(v) { fps = n; },
                                    "q" => if let Ok(n) = u8::from_str(v) { quality = n; },
                                    _ => {}
                                }
                            }
                        }
                    }
                    width = width.clamp(64, 4096);
                    height = height.clamp(36, 2160);
                    fps = fps.clamp(1, 60);
                    quality = quality.clamp(30, 95);
                    #[cfg(feature = "macos-capture")]
                    {
                        let mut rx_frames = capture::macos::spawn(fps);
                        tokio::spawn(async move {
                            while let Some(rgb) = rx_frames.recv().await {
                                let mut img = rgb;
                                let (w0, h0) = img.dimensions();
                                if w0 != width || h0 != height {
                                    img = image::imageops::resize(&img, width, height, image::imageops::FilterType::Triangle);
                                }
                                let mut out = Vec::new();
                                if let Ok(()) = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality).encode_image(&img) {
                                    let mut part = Vec::with_capacity(out.len() + 256);
                                    part.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                                    part.extend_from_slice(b"Content-Type: image/jpeg\r\n");
                                    part.extend_from_slice(format!("Content-Length: {}\r\n\r\n", out.len()).as_bytes());
                                    part.extend_from_slice(&out);
                                    part.extend_from_slice(b"\r\n");
                                    if tx.send(Ok(Bytes::from(part))).await.is_err() { break; }
                                } else { break; }
                            }
                        });
                    }
                    #[cfg(not(feature = "macos-capture"))]
                    {
                        {
                            let jpeg: Vec<u8> = synthesize_jpeg_frame(width, height, 0, quality).unwrap_or_default();
                            let mut part = Vec::with_capacity(jpeg.len() + 256);
                            part.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                            part.extend_from_slice(b"Content-Type: image/jpeg\r\n");
                            part.extend_from_slice(format!("Content-Length: {}\r\n\r\n", jpeg.len()).as_bytes());
                            part.extend_from_slice(&jpeg);
                            part.extend_from_slice(b"\r\n");
                            let _ = tx.send(Ok(Bytes::from(part))).await;
                        }
                        let mut t = tokio::time::interval(std::time::Duration::from_millis((1000 / fps.max(1)) as u64));
                        let start = std::time::Instant::now();
                        tokio::spawn(async move {
                            let mut frame_idx: u64 = 1;
                            loop {
                                t.tick().await;
                                frame_idx = frame_idx.wrapping_add(1);
                                let jpeg: Vec<u8> = synthesize_jpeg_frame(width, height, frame_idx, quality).unwrap_or_default();
                                let mut part = Vec::with_capacity(jpeg.len() + 256);
                                part.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
                                part.extend_from_slice(b"Content-Type: image/jpeg\r\n");
                                part.extend_from_slice(format!("Content-Length: {}\r\n\r\n", jpeg.len()).as_bytes());
                                part.extend_from_slice(&jpeg);
                                part.extend_from_slice(b"\r\n");
                                if tx.send(Ok(Bytes::from(part))).await.is_err() { break; }
                                if start.elapsed().as_secs() > 30 { break; }
                            }
                        });
                    }
                    let stream = ReceiverStream::new(rx).map(|r| r.map(Frame::data));
                    let body = StreamBody::new(stream).boxed();
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", format!("multipart/x-mixed-replace; boundary={}", boundary))
                        .body(body)
                        .unwrap()
                }
            },
            (&Method::GET, "/status") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let running = client.is_running().await;
                    let ctrl: Option<serde_json::Value> = {
                        #[cfg(feature = "quic-ctrl")] {
                            Some(serde_json::to_value(client.ctrl_status().await).unwrap())
                        }
                        #[cfg(not(feature = "quic-ctrl"))] {
                            None
                        }
                    };
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
            (&Method::GET, "/ctrl/config") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let ctrl: Option<serde_json::Value> = {
                        #[cfg(feature = "quic-ctrl")] {
                            Some(serde_json::to_value(client.ctrl_status().await).unwrap())
                        }
                        #[cfg(not(feature = "quic-ctrl"))] {
                            None
                        }
                    };
                    let body = serde_json::to_vec(&CtrlConfigPayload { ctrl }).unwrap();
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header(hyper::header::CACHE_CONTROL, "no-store")
                        .body(Full::new(Bytes::from(body)).boxed())
                        .unwrap()
                }
            },
            // Server-Sent Events for push updates
            (&Method::GET, "/events") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let (tx, rx) = mpsc::channel::<Result<Bytes, Infallible>>(32);
                    let c2 = client.clone();
                    let rl_p = rl_post.clone();
                    let rl_s = rl_stream.clone();
                    let cons = consent_allowed.clone();
                    tokio::spawn(async move {
                        // helper to read rate stats
                        async fn rates(rl_p: &Arc<RateLimiter>, rl_s: &Arc<RateLimiter>) -> RateStatus {
                            RateStatus {
                                post_calls: *rl_p.calls.lock().await,
                                post_denied: *rl_p.denied.lock().await,
                                stream_calls: *rl_s.calls.lock().await,
                                stream_denied: *rl_s.denied.lock().await,
                            }
                        }
                        // helper to send a status event
                        async fn send_status(tx: &mpsc::Sender<Result<Bytes, Infallible>>, running: bool, ctrl: Option<serde_json::Value>, rates: RateStatus, consent: bool) {
                            let payload = serde_json::json!({
                                "running": running,
                                "ctrl": ctrl,
                                "rates": {"post_calls": rates.post_calls, "post_denied": rates.post_denied, "stream_calls": rates.stream_calls, "stream_denied": rates.stream_denied},
                                "consent_allowed": consent
                            });
                            let line = format!("event: status\ndata: {}\n\n", payload);
                            let _ = tx.send(Ok(Bytes::from(line))).await;
                        }
                        // initial snapshot
                        let mut rx_evt = c2.subscribe();
                        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
                        // send an immediate snapshot so clients receive data without waiting
                        {
                            let running = c2.is_running().await;
                            let ctrl: Option<serde_json::Value> = {
                                #[cfg(feature = "quic-ctrl")] { Some(serde_json::to_value(c2.ctrl_status().await).unwrap()) }
                                #[cfg(not(feature = "quic-ctrl"))] { None }
                            };
                            let rs = rates(&rl_p, &rl_s).await;
                            let consent = *cons.lock().await;
                            send_status(&tx, running, ctrl, rs, consent).await;
                        }
                        loop {
                            tokio::select! {
                                _ = ticker.tick() => {
                                    let running = c2.is_running().await;
                                    let ctrl: Option<serde_json::Value> = {
                                        #[cfg(feature = "quic-ctrl")] { Some(serde_json::to_value(c2.ctrl_status().await).unwrap()) }
                                        #[cfg(not(feature = "quic-ctrl"))] { None }
                                    };
                                    let rs = rates(&rl_p, &rl_s).await;
                                    let consent = *cons.lock().await;
                                    send_status(&tx, running, ctrl, rs, consent).await;
                                    // heartbeat comment
                                    let _ = tx.send(Ok(Bytes::from_static(b":\n\n"))).await;
                                }
                                evt = rx_evt.recv() => {
                                    match evt {
                                        Ok(core::ClientEvent::Started) | Ok(core::ClientEvent::Stopped)
                                        | Ok(core::ClientEvent::ConsentRequired { .. })
                                        | Ok(core::ClientEvent::CtrlConnected)
                                        | Ok(core::ClientEvent::CtrlDisconnected { .. })
                                        | Ok(core::ClientEvent::CtrlAuthRenewed)
                                        | Ok(core::ClientEvent::SessionIncoming { .. }) => {
                                            let running = c2.is_running().await;
                                            let ctrl: Option<serde_json::Value> = {
                                                #[cfg(feature = "quic-ctrl")] { Some(serde_json::to_value(c2.ctrl_status().await).unwrap()) }
                                                #[cfg(not(feature = "quic-ctrl"))] { None }
                                            };
                                            let rs = rates(&rl_p, &rl_s).await;
                                            let consent = *cons.lock().await;
                                            send_status(&tx, running, ctrl, rs, consent).await;
                                        }
                                        Err(_) => break,
                                    }
                                }
                            }
                        }
                    });
                    let stream = ReceiverStream::new(rx).map(|r| r.map(Frame::data));
                    let body = StreamBody::new(stream).boxed();
                    Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "text/event-stream")
                        .header(hyper::header::CACHE_CONTROL, "no-store")
                        .header(hyper::header::CONNECTION, "keep-alive")
                        .body(body)
                        .unwrap()
                }
            },
            (&Method::GET, "/displays") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let list = list_displays();
                    let sel = disp_state.selected.lock().await.clone();
                    let body = serde_json::to_vec(&serde_json::json!({
                        "selected": sel,
                        "displays": list,
                    })).unwrap();
                    Response::builder().status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .header(hyper::header::CACHE_CONTROL, "no-store")
                        .body(Full::new(Bytes::from(body)).boxed()).unwrap()
                }
            },
            (&Method::POST, "/displays/select") => {
                if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else {
                    let body = match req.collect().await { Ok(b) => b.to_bytes(), Err(_) => Bytes::new() };
                    let id = serde_json::from_slice::<serde_json::Value>(&body)
                        .ok()
                        .and_then(|v| v.get("id").and_then(|x| x.as_u64()))
                        .map(|n| n as u32);
                    if let Some(id) = id {
                        let ids: Vec<u32> = list_displays().into_iter().map(|d| d.id).collect();
                        if ids.contains(&id) {
                            *disp_state.selected.lock().await = Some(id);
                            Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap()
                        } else {
                            Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap()
                        }
                    } else {
                        Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap()
                    }
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
            (&Method::GET, p) if static_dir.is_some() => {
                let dir = static_dir.as_ref().unwrap();
                let rel = p.trim_start_matches('/');
                if rel.contains("..") {
                    Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if rel.is_empty() {
                    match serve_static(dir, "index.html").await {
                        Ok((body, content_type)) => Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", content_type)
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
                    match serve_static(dir, rel).await {
                        Ok((body, content_type)) => Response::builder()
                            .status(StatusCode::OK)
                            .header("content-type", content_type)
                            .header(hyper::header::CACHE_CONTROL, "public, max-age=300")
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
            (&Method::POST, "/input/mouse") => {
                if !rl_post.allow(1.0).await {
                    Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !*consent_allowed.lock().await {
                    Response::builder().status(StatusCode::FORBIDDEN).body(Full::new(Bytes::from_static(b"{}")).boxed()).unwrap()
                } else {
                    let whole = req.collect().await;
                    match whole {
                        Ok(b) => {
                            let body = b.to_bytes();
                            match serde_json::from_slice::<MouseInput>(&body) {
                                Ok(mi) => {
                                    let ok = inject_mouse(mi, &disp_state).await;
                                    if ok { Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap() }
                                    else { Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap() }
                                }
                                Err(_) => Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap(),
                            }
                        }
                        Err(_) => Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap(),
                    }
                }
            },
            (&Method::POST, "/input/key") => {
                if !rl_post.allow(1.0).await {
                    Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !authorized(&req) {
                    Response::builder().status(StatusCode::UNAUTHORIZED).body(Full::new(Bytes::new()).boxed()).unwrap()
                } else if !*consent_allowed.lock().await {
                    Response::builder().status(StatusCode::FORBIDDEN).body(Full::new(Bytes::from_static(b"{}")).boxed()).unwrap()
                } else {
                    let whole = req.collect().await;
                    match whole {
                        Ok(b) => {
                            let body = b.to_bytes();
                            match serde_json::from_slice::<KeyInput>(&body) {
                                Ok(ki) => {
                                    let ok = inject_key(ki).await;
                                    if ok { Response::builder().status(StatusCode::NO_CONTENT).body(Full::new(Bytes::new()).boxed()).unwrap() }
                                    else { Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR).body(Full::new(Bytes::new()).boxed()).unwrap() }
                                }
                                Err(_) => Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap(),
                            }
                        }
                        Err(_) => Response::builder().status(StatusCode::BAD_REQUEST).body(Full::new(Bytes::new()).boxed()).unwrap(),
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
            _ => Response::builder().status(StatusCode::NOT_FOUND).body(Full::new(Bytes::new()).boxed()).unwrap(),
        };
    // Attach CORS based on allowlist and add security headers
    let headers = response.headers_mut();
    // Security headers
    headers.insert(hyper::header::REFERRER_POLICY, "no-referrer".parse().unwrap());
    headers.insert(hyper::header::HeaderName::from_static("x-content-type-options"), "nosniff".parse().unwrap());
    headers.insert(
        hyper::header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; connect-src 'self'; frame-ancestors 'none'; object-src 'none'; base-uri 'self'"
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

    #[derive(Debug, Deserialize, Clone, Copy)]
    #[serde(rename_all = "snake_case")]
    enum MouseButton { Left, Right, Middle }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct MouseInput {
        // absolute position in screen coordinates (pixels); optional
        x: Option<f64>,
        y: Option<f64>,
    // optional frame size the coordinates were derived from (for auto scaling)
    frame_w: Option<u32>,
    frame_h: Option<u32>,
    // optional explicit scale overrides
    scale_x: Option<f64>,
    scale_y: Option<f64>,
    // optional explicit display id to target (overrides selection state)
    display_id: Option<u32>,
        // wheel deltas; optional
        wheel_x: Option<f64>,
        wheel_y: Option<f64>,
        // button and press state for clicks; optional
        button: Option<MouseButton>,
        down: Option<bool>,
    }

    #[derive(Debug, Deserialize)]
    #[allow(dead_code)]
    struct KeyInput {
        // either a printable text (single character) or a key code name
        text: Option<String>,
        key: Option<String>,
        down: Option<bool>,
    }

    async fn inject_mouse(mi: MouseInput, disp_state: &Arc<DisplayState>) -> bool {
        #[cfg(all(target_os = "macos", feature = "macos-input"))]
        {
            use core_graphics::event::{CGEvent, CGEventType, CGMouseButton};
            use core_graphics::event_source::CGEventSource;
            use core_graphics::geometry::CGPoint;
            // Helper to scale coordinates if frame size or explicit scale provided
            let scale_coords = |x: f64, y: f64, mi: &MouseInput| -> (f64, f64) {
                if let (Some(sx), Some(sy)) = (mi.scale_x, mi.scale_y) { return (x * sx, y * sy); }
                if let (Some(fw), Some(fh)) = (mi.frame_w, mi.frame_h) {
                    // scale frame coords to main display pixel size
                    unsafe {
                        let did = CGMainDisplayID();
                        let dw = CGDisplayPixelsWide(did) as f64;
                        let dh = CGDisplayPixelsHigh(did) as f64;
                        if fw > 0 && fh > 0 { return (x * (dw / fw as f64), y * (dh / fh as f64)); }
                    }
                }
                (x, y)
            };
            // Determine target display id and origin offset
            let target_display = if let Some(id) = mi.display_id { Some(id) } else { disp_state.selected.lock().await.clone() };
            let origin = target_display.and_then(|id| get_display_origin(id));
            // Create event source after awaiting any locks to avoid holding non-Send across await
            let src = CGEventSource::new(core_graphics::event_source::CGEventSourceStateID::HIDSystemState).ok();
            // Move
            if let (Some(x), Some(y)) = (mi.x, mi.y) {
                let (x, y) = scale_coords(x, y, &mi);
                let (x, y) = if let Some((ox, oy)) = origin { (x + ox, y + oy) } else { (x, y) };
                if let Some(src) = src.clone() {
                    if let Ok(ev) = CGEvent::new_mouse_event(src, CGEventType::MouseMoved, CGPoint::new(x, y), CGMouseButton::Left) {
                        let _ = ev.post(core_graphics::event::CGEventTapLocation::HID);
                    }
                }
            }
            // Wheel
            if mi.wheel_x.unwrap_or(0.0) != 0.0 || mi.wheel_y.unwrap_or(0.0) != 0.0 {
                unsafe {
                    #[allow(non_camel_case_types)]
                    #[repr(i32)]
                    enum ScrollUnit { Pixel = 0 }
                    #[link(name = "ApplicationServices", kind = "framework")]
                    extern "C" {
                        fn CGEventCreateScrollWheelEvent(
                            source: *const c_void,
                            units: ScrollUnit,
                            wheelCount: u32,
                            wheel1: i32,
                            ...,
                        ) -> *mut c_void;
                        fn CGEventPost(tap: core_graphics::event::CGEventTapLocation, event: *mut c_void);
                        fn CFRelease(cf: *const c_void);
                    }
                    let dy = mi.wheel_y.unwrap_or(0.0);
                    let dx = mi.wheel_x.unwrap_or(0.0);
                    // DOM delta positive typically scrolls down/right; invert Y to match typical CG semantics
                    let wy = if dy == 0.0 { 0 } else { (-dy.round()) as i32 };
                    let wx = if dx == 0.0 { 0 } else { (dx.round()) as i32 };
                    if wy != 0 || wx != 0 {
                        let ev = if wx != 0 {
                            CGEventCreateScrollWheelEvent(std::ptr::null(), ScrollUnit::Pixel, 2, wy, wx)
                        } else {
                            CGEventCreateScrollWheelEvent(std::ptr::null(), ScrollUnit::Pixel, 1, wy)
                        };
                        if !ev.is_null() {
                            CGEventPost(core_graphics::event::CGEventTapLocation::HID, ev);
                            CFRelease(ev as *const c_void);
                        }
                    }
                }
            }
            // Button
            if let Some(button) = mi.button {
                let down = mi.down.unwrap_or(true);
                let (cg_btn, ev_down, ev_up) = match button {
                    MouseButton::Left => (CGMouseButton::Left, CGEventType::LeftMouseDown, CGEventType::LeftMouseUp),
                    MouseButton::Right => (CGMouseButton::Right, CGEventType::RightMouseDown, CGEventType::RightMouseUp),
                    MouseButton::Middle => (CGMouseButton::Center, CGEventType::OtherMouseDown, CGEventType::OtherMouseUp),
                };
                if let Some(src) = src.clone() {
                    let ty = if down { ev_down } else { ev_up };
                    let (x, y) = {
                        let x0 = mi.x.unwrap_or(0.0);
                        let y0 = mi.y.unwrap_or(0.0);
                        scale_coords(x0, y0, &mi)
                    };
                    let (x, y) = if let Some((ox, oy)) = origin { (x + ox, y + oy) } else { (x, y) };
                    if let Ok(ev) = CGEvent::new_mouse_event(src, ty, CGPoint::new(x, y), cg_btn) {
                        let _ = ev.post(core_graphics::event::CGEventTapLocation::HID);
                    }
                }
            }
            true
        }
        #[cfg(not(all(target_os = "macos", feature = "macos-input")))]
        {
            let _ = mi; // not supported on this platform/feature set
            let _ = disp_state;
            true
        }
    }

    async fn inject_key(ki: KeyInput) -> bool {
        #[cfg(all(target_os = "macos", feature = "macos-input"))]
        {
            use core_graphics::event::CGEvent;
            use core_graphics::event_source::CGEventSource;
            let src = match CGEventSource::new(core_graphics::event_source::CGEventSourceStateID::HIDSystemState) { Ok(s) => s, Err(_) => return false };
            // If text provided, type it via set_string on key down
            if let Some(text) = ki.text.clone() {
                if ki.down.unwrap_or(true) {
                    if let Ok(ev) = CGEvent::new_keyboard_event(src.clone(), 0, true) {
                        ev.set_string(&text);
                        let _ = ev.post(core_graphics::event::CGEventTapLocation::HID);
                        return true;
                    }
                }
                return true;
            }
            // Else handle keycodes from key names
            if let Some(kname) = ki.key.as_deref() {
                if let Some(code) = map_key_name_to_code(kname) {
                    let down = ki.down.unwrap_or(true);
                    if let Ok(ev) = CGEvent::new_keyboard_event(src, code, down) {
                        let _ = ev.post(core_graphics::event::CGEventTapLocation::HID);
                        return true;
                    }
                }
            }
            true
        }
        #[cfg(not(all(target_os = "macos", feature = "macos-input")))]
        {
            let _ = ki;
            true
        }
    }

    #[cfg(all(target_os = "macos", feature = "macos-input"))]
    fn map_key_name_to_code(name: &str) -> Option<u16> {
        // Handle common named keys and single characters (US layout)
        let lower = name.to_lowercase();
        match name {
            "Enter" | "Return" => Some(0x24),
            "Escape" | "Esc" => Some(0x35),
            "Backspace" => Some(0x33),
            "Delete" => Some(0x75),
            "Insert" => Some(0x72), // Help
            "Tab" => Some(0x30),
            "Space" => Some(0x31),
            "ArrowLeft" => Some(0x7B),
            "ArrowRight" => Some(0x7C),
            "ArrowDown" => Some(0x7D),
            "ArrowUp" => Some(0x7E),
            "Home" => Some(0x73),
            "End" => Some(0x77),
            "PageUp" => Some(0x74),
            "PageDown" => Some(0x79),
            "Shift" => Some(0x38),
            "Control" => Some(0x3B),
            "Alt" | "Option" => Some(0x3A),
            "Meta" | "Command" | "Super" | "Windows" => Some(0x37),
            // Function keys
            "F1" => Some(0x7A),
            "F2" => Some(0x78),
            "F3" => Some(0x63),
            "F4" => Some(0x76),
            "F5" => Some(0x60),
            "F6" => Some(0x61),
            "F7" => Some(0x62),
            "F8" => Some(0x64),
            "F9" => Some(0x65),
            "F10" => Some(0x6D),
            "F11" => Some(0x67),
            "F12" => Some(0x6F),
            "PrintScreen" => Some(0x69), // F13
            // Media keys (may require special handling in macOS settings)
            "AudioVolumeUp" => Some(0x48),
            "AudioVolumeDown" => Some(0x49),
            "AudioVolumeMute" | "VolumeMute" => Some(0x4A),
            _ => {
                // Single character mapping for letters and digits
                if lower.len() == 1 {
                    let c = lower.chars().next().unwrap();
                    return Some(match c {
                        'a' => 0x00,
                        's' => 0x01,
                        'd' => 0x02,
                        'f' => 0x03,
                        'h' => 0x04,
                        'g' => 0x05,
                        'z' => 0x06,
                        'x' => 0x07,
                        'c' => 0x08,
                        'v' => 0x09,
                        'b' => 0x0B,
                        'q' => 0x0C,
                        'w' => 0x0D,
                        'e' => 0x0E,
                        'r' => 0x0F,
                        'y' => 0x10,
                        't' => 0x11,
                        '1' => 0x12,
                        '2' => 0x13,
                        '3' => 0x14,
                        '4' => 0x15,
                        '6' => 0x16,
                        '5' => 0x17,
                        '=' => 0x18,
                        '9' => 0x19,
                        '7' => 0x1A,
                        '-' => 0x1B,
                        '8' => 0x1C,
                        '0' => 0x1D,
                        ']' => 0x1E,
                        'o' => 0x1F,
                        'u' => 0x20,
                        '[' => 0x21,
                        'i' => 0x22,
                        'p' => 0x23,
                        'l' => 0x25,
                        'j' => 0x26,
                        '\'' => 0x27,
                        'k' => 0x28,
                        ';' => 0x29,
                        '\\' => 0x2A,
                        ',' => 0x2B,
                        '/' => 0x2C,
                        'n' => 0x2D,
                        'm' => 0x2E,
                        '.' => 0x2F,
                        '`' => 0x32,
                        _ => return None,
                    });
                }
                None
            }
        }
    }

    async fn serve_static(dir: &Arc<PathBuf>, rel: &str) -> Result<(Bytes, &'static str), StatusCode> {
        // Join safely
        let path = Path::new(&**dir).join(rel);
        // Simple content-type mapping
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
            "woff" => "font/woff",
            "woff2" => "font/woff2",
            "ttf" => "font/ttf",
            "otf" => "font/otf",
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
                            // clone per-connection so we can move into the service closure repeatedly
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

    #[derive(Debug, Serialize, Clone, Copy)]
    struct DisplayInfo {
        id: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        is_main: bool,
    }

    fn list_displays() -> Vec<DisplayInfo> {
        #[cfg(all(target_os = "macos", feature = "macos-input"))]
        {
            unsafe {
                extern "C" {
                    fn CGGetActiveDisplayList(max: u32, active: *mut u32, count: *mut u32) -> i32;
                    fn CGDisplayBounds(display: u32) -> CGR;
                }
                let mut count: u32 = 0;
                let _ = CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count);
                let n = count.min(16);
                let mut buf = vec![0u32; n as usize];
                let _ = CGGetActiveDisplayList(n, buf.as_mut_ptr(), &mut count);
                let main_id = CGMainDisplayID();
                buf.truncate(count as usize);
                buf.into_iter().map(|id| {
                    let r = CGDisplayBounds(id);
                    let (x, y, w, h) = (r.origin.x as i32, r.origin.y as i32, r.size.width as u32, r.size.height as u32);
                    DisplayInfo { id, x, y, width: w, height: h, is_main: id==main_id }
                }).collect()
            }
        }
        #[cfg(not(all(target_os = "macos", feature = "macos-input")))]
        { vec![] }
    }

    #[cfg(all(target_os = "macos", feature = "macos-input"))]
    fn get_display_origin(id: u32) -> Option<(f64, f64)> {
        unsafe {
            extern "C" { fn CGDisplayBounds(display: u32) -> CGR; }
            let r = CGDisplayBounds(id);
            Some((r.origin.x as f64, r.origin.y as f64))
        }
    }

    #[cfg(not(all(target_os = "macos", feature = "macos-input")))]
    fn get_display_origin(_id: u32) -> Option<(f64, f64)> { None }

    // Clipboard helpers with in-memory fallback
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
        // Fallback to cache
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

    #[cfg(not(feature = "macos-capture"))]
    fn synthesize_jpeg_frame(width: u32, height: u32, idx: u64, quality: u8) -> Result<Vec<u8>, image::ImageError> {
        use image::{RgbImage, Rgb, codecs::jpeg::JpegEncoder};
        let mut img = RgbImage::from_pixel(width, height, Rgb([20, 20, 20]));
        // moving rectangle
        let t = (idx % 200) as i32;
        let x0 = (t * 2 % (width as i32)).max(0) as u32;
        let y0 = (t % (height as i32)).max(0) as u32;
        let w = (width / 6).max(10);
        let h = (height / 6).max(10);
        for y in y0..(y0 + h).min(height) {
            for x in x0..(x0 + w).min(width) {
                img.put_pixel(x, y, Rgb([200, 80, 40]));
            }
        }
        let mut out = Vec::new();
    let mut enc = JpegEncoder::new_with_quality(&mut out, quality);
        enc.encode_image(&img)?;
        Ok(out)
    }
}
