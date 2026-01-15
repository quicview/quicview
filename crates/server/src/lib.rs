use anyhow::Result;
use bytes::Bytes;
use http_body_util::Full;
use config::QuicViewConfig;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Method, Request, Response, StatusCode};
use serde::Serialize;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::sync::Notify;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

// QUIC server module
#[cfg(feature = "quic-server")]
pub mod quic_server;

// Re-export QUIC server types when feature is enabled
#[cfg(feature = "quic-server")]
pub use quic_server::{
    QuicServerConfig, QuicServerEvent, QuicServerHandle,
    start_quic_server, ScreenCapturer, InputInjector,
};

#[derive(Debug, Clone, Serialize)]
struct HealthPayload {
    status: &'static str,
}

struct ReadyState {
    expected_children: AtomicUsize,
    running_children: AtomicUsize,
}

impl ReadyState {
    fn new() -> Self {
        Self { expected_children: AtomicUsize::new(0), running_children: AtomicUsize::new(0) }
    }
    fn set_expected(&self, n: usize) {
        self.expected_children.store(n, Ordering::SeqCst);
    }
    fn child_started(&self) {
        self.running_children.fetch_add(1, Ordering::SeqCst);
    }
    fn is_ready(&self) -> bool {
        let exp = self.expected_children.load(Ordering::SeqCst);
        let run = self.running_children.load(Ordering::SeqCst);
        run >= exp
    }
}

struct HealthSvc {
    ready: Arc<ReadyState>,
}

impl Service<Request<Incoming>> for HealthSvc {
    type Response = Response<Full<Bytes>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let ready_arc = self.ready.clone();
        Box::pin(async move {
        let not_found = || {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
                    .unwrap()
            };
            match (req.method(), req.uri().path()) {
                (&Method::GET, "/health") => {
                    let body = serde_json::to_vec(&HealthPayload { status: "ok" }).unwrap();
                    Ok(Response::builder()
                        .status(StatusCode::OK)
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(body)))
                        .unwrap())
                }
                (&Method::GET, "/ready") => {
                    let ready = ready_arc.is_ready();
                    let body = serde_json::to_vec(&HealthPayload { status: if ready { "ok" } else { "not-ready" } }).unwrap();
                    Ok(Response::builder()
                        .status(if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE })
                        .header("content-type", "application/json")
                        .body(Full::new(Bytes::from(body)))
                        .unwrap())
                }
                _ => Ok(not_found()),
            }
        })
    }
}

pub struct ServerHandle {
    pub addr: SocketAddr,
    join: JoinHandle<()>,
    shutdown: Arc<Notify>,
}

impl ServerHandle {
    pub async fn wait(self) {
        let _ = self.join.await;
    }

    pub async fn shutdown(self) {
        self.shutdown.notify_waiters();
        let _ = self.join.await;
    }

    /// Signal shutdown without consuming the handle (useful for orchestrating multiple subsystems).
    pub fn signal_shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

/// Run the minimal HTTP health server. Binds to `bind_addr`.
pub(crate) async fn run_health_server(bind_addr: SocketAddr, ready: Arc<ReadyState>, shutdown: Arc<Notify>) -> Result<ServerHandle> {
    let listener = TcpListener::bind(bind_addr).await?;
    let local = listener.local_addr()?;
    info!(addr = %local, "health server listening");

    let shutdown_task = shutdown.clone();
    let join = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_task.notified() => {
                    info!("health server: shutdown signaled");
                    break;
                }
                accept_result = listener.accept() => {
                    let Ok((stream, peer)) = accept_result else { break };
                    info!(%peer, "accepted");
                    let io = hyper_util::rt::TokioIo::new(stream);
                    let svc = HealthSvc { ready: ready.clone() };
                    tokio::spawn(async move {
                        if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                            error!(error = %e, "serve error");
                        }
                    });
                }
            }
        }
        info!("health server: stopped");
    });
    Ok(ServerHandle { addr: local, join, shutdown })
}

/// Entrypoint to start all server subsystems based on config.
/// Returns a join handle for the health server (more handles can be added later).
pub async fn run(cfg: &QuicViewConfig) -> Result<ServerHandle> {
    // Init tracing once with an env filter if not already set up.
    // Use `RUST_LOG` if present; otherwise default to `info` for this crate.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    // Health bind: use `server.health_bind:server.health_port` or default 127.0.0.1:0
    let host = cfg
        .server
        .health_bind
        .clone()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let port = cfg.server.health_port.unwrap_or(0);
    let addr = format!("{host}:{port}").parse::<SocketAddr>()?;
    let ready = Arc::new(ReadyState::new());
    let shutdown = Arc::new(Notify::new());

    // No external child processes by default; readiness reflects our own listeners.
    ready.set_expected(1);
    ready.child_started();

    let handle = run_health_server(addr, ready.clone(), shutdown.clone()).await?;
    info!(addr = %handle.addr, "server started: health and readiness endpoints active");

    Ok(ServerHandle { shutdown, ..handle })
}

/// Probe a TCP host:port with a per-attempt timeout, retrying with backoff.
/// Returns true if a connection succeeds at least once.
#[allow(dead_code)]
pub async fn wait_for_port(host: &str, port: u16, per_attempt: std::time::Duration, retries: usize, backoff: std::time::Duration) -> bool {
    use tokio::time::{timeout, sleep};
    for _ in 0..retries {
        let addr = format!("{}:{}", host, port);
        match timeout(per_attempt, tokio::net::TcpStream::connect(addr)).await {
            Ok(Ok(_stream)) => return true,
            _ => sleep(backoff).await,
        }
    }
    false
}
