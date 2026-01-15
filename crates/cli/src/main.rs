use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use config::QuicViewConfig;
use proto::{handshake_tcp, handshake_tls, parse_host_port, probe_tcp, probe_tls};
use server::run as run_server;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use client::core as client_core;
#[cfg(feature = "http-ui")]
use client::http_ui as client_http;
use transport::quic_ctrl::{self, Cmd as CtrlCmd, ServerSignal};

/// `QuicView` CLI
#[derive(Debug, Parser)]
#[command(author, version, about = "QuicView: enterprise harness for RustDesk", long_about = None)]
struct Cli {
    /// Path to `QuicView` config file
    #[arg(short, long, default_value = "quicview.yaml", global = true)]
    config: String,

    /// Optional path to a file containing the HMAC key for client-handshake (overrides `DLNK_KEY`)
    #[arg(long, global = true)]
    key_from_file: Option<String>,

    /// Override rendezvous endpoint `host[:port]` for quick tests (e.g., 127.0.0.1:21116)
    #[arg(long, global = true)]
    rendezvous: Option<String>,

    /// Force-disable TLS (overrides config.server.tls)
    #[arg(long, global = true)]
    no_tls: bool,

    /// Override SNI for TLS (defaults to host portion of --rendezvous or config)
    #[arg(long, global = true)]
    sni: Option<String>,

    /// Provide a PEM file with a CA certificate chain to trust for TLS (in addition to system roots)
    #[arg(long, global = true)]
    ca_file: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Validate a `QuicView` config file
    Validate,
    /// Print an upstream-compatible summary (for scripting)
    Show,
    /// Launch the QuicView server harness (health/ready endpoints; transitional)
    LaunchServer {
        /// Parse config and exit without starting listeners
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Override health port (keeps bind IP from config or defaults to 127.0.0.1)
        #[arg(long)]
        health_port: Option<u16>,
    },
    /// Probe configured rendezvous/relay ports and report status
    Health,
    /// Quick client probe to rendezvous (TCP and optional TLS)
    ProbeClient {
        /// Timeout in milliseconds (default: 1500)
        #[arg(short = 't', long = "timeout", default_value_t = 1500)]
        timeout_ms: u64,
    },
    /// Attempt a minimal custom handshake (`QuicView` original protocol) and report result
    ClientHandshake {
        /// Timeout in milliseconds (default: 3000)
        #[arg(short = 't', long = "timeout", default_value_t = 3000)]
        timeout_ms: u64,
    },
    /// Start the client core with a local HTTP control surface
    Client {
        /// Bind IP (default 127.0.0.1)
        #[arg(long)]
        bind: Option<IpAddr>,
        /// Port (0 for random)
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Start the client core on launch
        #[arg(long, default_value_t = false)]
        start: bool,
        /// Bearer token to require for POST actions (e.g., start/stop). If omitted, POSTs are open.
        #[arg(long)]
        auth_token: Option<String>,
        /// Open the browser to the local web UI after starting the server
        #[arg(long, default_value_t = false)]
        open: bool,
        /// Serve static files from this directory (e.g., built Leptos assets)
        #[arg(long)]
        static_dir: Option<std::path::PathBuf>,
        /// Allow binding to non-localhost addresses (use with caution)
        #[arg(long, default_value_t = false)]
        allow_external: bool,
        /// Default MJPEG width (can be overridden via /stream.mjpeg?w=)
        #[arg(long, default_value_t = 320)]
        mjpeg_width: u32,
        /// Default MJPEG height (can be overridden via /stream.mjpeg?h=)
        #[arg(long, default_value_t = 180)]
        mjpeg_height: u32,
        /// Default MJPEG FPS (can be overridden via /stream.mjpeg?fps=)
        #[arg(long, default_value_t = 5)]
        mjpeg_fps: u32,
        /// Default MJPEG JPEG quality 30-95 (can be overridden via /stream.mjpeg?q=)
        #[arg(long, default_value_t = 70)]
        mjpeg_quality: u8,
        /// Allowed CORS origins (CSV); if omitted, CORS is disabled by default
        #[arg(long)]
        allowed_origins: Option<String>,
        /// POST endpoints rate limit: burst
        #[arg(long, default_value_t = 30.0)]
        post_burst: f64,
        /// POST endpoints rate limit: refill per second
        #[arg(long, default_value_t = 15.0)]
        post_refill: f64,
        /// Stream endpoint rate limit: burst
        #[arg(long, default_value_t = 5.0)]
        stream_burst: f64,
        /// Stream endpoint rate limit: refill per second
        #[arg(long, default_value_t = 2.0)]
        stream_refill: f64,
        /// Initial consent allowed state (true/false)
        #[arg(long, default_value_t = true)]
        consent_allowed: bool,
        /// QUIC control server address (e.g., 127.0.0.1:4433); enables control channel when set
        #[arg(long)]
        ctrl_addr: Option<String>,
        /// QUIC control bearer token for handshake/auth
        #[arg(long)]
        ctrl_token: Option<String>,
        /// QUIC heartbeat ping interval (seconds)
        #[arg(long, default_value_t = 10)]
        ctrl_ping_secs: u64,
        /// QUIC reconnect backoff base in milliseconds
        #[arg(long, default_value_t = 500)]
        ctrl_backoff_base_ms: u64,
        /// QUIC reconnect backoff max cap in milliseconds
        #[arg(long, default_value_t = 32000)]
        ctrl_backoff_max_ms: u64,
        /// QUIC TLS mode: one of insecure, system, pin:<hex>, tofu
        #[arg(long)]
        ctrl_tls: Option<String>,
        /// QUIC TLS SNI (required for system/pin/tofu modes)
        #[arg(long)]
        ctrl_sni: Option<String>,
        /// QUIC TLS extra CA PEM file (system mode)
        #[arg(long)]
        ctrl_ca_file: Option<std::path::PathBuf>,
        /// QUIC TLS TOFU pin cache file path (read/write)
        #[arg(long)]
        ctrl_tofu_pin_file: Option<std::path::PathBuf>,
    },
    /// Run a minimal QUIC control server (dev/testing)
    CtrlServer {
        /// Bind address (default 127.0.0.1:4433)
        #[arg(long)]
        bind: Option<SocketAddr>,
        /// Required bearer token clients must present in Hello
        #[arg(long)]
        token: String,
    },
}

#[allow(clippy::too_many_lines, clippy::similar_names)]
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate => {
            let cfg = QuicViewConfig::load_from_file(&cli.config)
                .with_context(|| format!("loading config from {}", cli.config))?;
            // Print deprecation warnings if legacy fields are used
            for w in cfg.server.deprecation_warnings() {
                eprintln!("warning: {}", w);
            }
            println!(
                "OK: config loaded. host={}, port={}, tls={}",
                cfg.server.effective_host(), cfg.server.effective_port(), cfg.server.effective_tls_enabled()
            );
        }
        Commands::LaunchServer { dry_run, health_port } => {
            let mut cfg = QuicViewConfig::load_from_file(&cli.config)
                .with_context(|| format!("loading config from {}", cli.config))?;
            if let Some(p) = health_port { cfg.server.health_port = Some(p); }
            if dry_run {
                println!(
                    "dry-run: health_bind={:?} health_port={:?}",
                    cfg.server.health_bind, cfg.server.health_port
                );
                return Ok(());
            }
            let handle = run_server(&cfg).await?;
            println!("server.started health={} (Ctrl+C to exit)", handle.addr);
            // Wait for Ctrl+C or SIGTERM (Unix), then shutdown gracefully.
            #[cfg(unix)]
            {
                use tokio::signal::unix::{signal, SignalKind};
                let mut term = signal(SignalKind::terminate())?;
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            #[cfg(not(unix))]
            {
                tokio::signal::ctrl_c().await?;
            }
            println!("server.shutting_down ...");
            handle.shutdown().await;
        }
        Commands::Show => {
            let cfg = QuicViewConfig::load_from_file(&cli.config)
                .with_context(|| format!("loading config from {}", cli.config))?;
            // Print deprecation warnings
            for w in cfg.server.deprecation_warnings() {
                eprintln!("warning: {}", w);
            }
            println!("host={}", cfg.server.effective_host());
            println!("port={}", cfg.server.effective_port());
            println!("tls={}", cfg.server.effective_tls_enabled());
            println!("require_consent={}", cfg.client_policy.require_consent);
            println!(
                "allow_input_control={}",
                cfg.client_policy.allow_input_control
            );
            println!("allow_clipboard={}", cfg.client_policy.allow_clipboard);
            println!(
                "allow_file_transfer={}",
                cfg.client_policy.allow_file_transfer
            );
        }
        Commands::Health => {
            let cfg = QuicViewConfig::load_from_file(&cli.config)
                .with_context(|| format!("loading config from {}", cli.config))?;
            // Probe the configured server endpoint
            let timeout = 600u64;
            let server_host = cli
                .rendezvous
                .clone()
                .unwrap_or_else(|| format!("{}:{}", cfg.server.effective_host(), cfg.server.effective_port()));
            let (host, port) = parse_host_port(&server_host, cfg.server.effective_port());
            let tcp_ok = probe_tcp(&host, port, timeout).await.is_ok();
            println!("server={host}:{port} {}", if tcp_ok { "up" } else { "down" });

            let tls_enabled = cfg.server.effective_tls_enabled() && !cli.no_tls;
            if tls_enabled {
                let cfg_sni = cfg.server.effective_tls_sni();
                let sni = cli
                    .sni
                    .as_deref()
                    .or(cfg_sni.as_deref())
                    .unwrap_or(host.as_str());
                let cfg_ca = cfg.server.effective_tls_ca_file();
                let ca_bytes = match cli.ca_file.as_deref().or(cfg_ca.as_deref()) {
                    Some(p) => std::fs::read(p).ok(),
                    None => None,
                };
                let tls_ok = probe_tls(&host, port, 800, Some(sni), ca_bytes.as_deref())
                    .await
                    .is_ok();
                println!("server.tls_handshake={host}:{port} {}", if tls_ok { "ok" } else { "fail" });
            }
        }
        Commands::ProbeClient { timeout_ms } => {
            let cfg = QuicViewConfig::load_from_file(&cli.config)
                .with_context(|| format!("loading config from {}", cli.config))?;
            let server = cli
                .rendezvous
                .clone()
                .unwrap_or_else(|| format!("{}:{}", cfg.server.effective_host(), cfg.server.effective_port()));
            let (host, port) = parse_host_port(&server, cfg.server.effective_port());
            let tcp = probe_tcp(&host, port, timeout_ms).await.is_ok();
            println!("client_probe.tcp={host}:{port} {}", if tcp { "ok" } else { "fail" });
            let tls_enabled = cfg.server.effective_tls_enabled() && !cli.no_tls;
            if tls_enabled {
                let cfg_sni = cfg.server.effective_tls_sni();
                let sni = cli
                    .sni
                    .as_deref()
                    .or(cfg_sni.as_deref())
                    .unwrap_or(host.as_str());
                let cfg_ca = cfg.server.effective_tls_ca_file();
                let ca_bytes = match cli.ca_file.as_deref().or(cfg_ca.as_deref()) {
                    Some(p) => std::fs::read(p).ok(),
                    None => None,
                };
                let tls = probe_tls(
                    &host,
                    port,
                    timeout_ms,
                    Some(sni),
                    ca_bytes.as_deref(),
                )
                .await
                .is_ok();
                println!("client_probe.tls={host}:{port} {}", if tls { "ok" } else { "fail" });
            }
        }
        Commands::ClientHandshake { timeout_ms } => {
            let cfg = QuicViewConfig::load_from_file(&cli.config)
                .with_context(|| format!("loading config from {}", cli.config))?;
            let server = cli
                .rendezvous
                .clone()
                .unwrap_or_else(|| format!("{}:{}", cfg.server.effective_host(), cfg.server.effective_port()));
            let (host, port) = parse_host_port(&server, cfg.server.effective_port());
            let auth_key = if let Some(path) = cli.key_from_file.as_deref() {
                match std::fs::read(path) {
                    Ok(bytes) => Some(String::from_utf8_lossy(&bytes).trim().to_string()),
                    Err(e) => {
                        eprintln!("failed to read key file {path}: {e}");
                        std::process::exit(6);
                    }
                }
            } else {
                // Check config auth_token or env
                cfg.server.auth_token.clone().or_else(|| std::env::var("DLNK_KEY").ok())
            };
            let tls_enabled = cfg.server.effective_tls_enabled() && !cli.no_tls;
            let res = if tls_enabled {
                let cfg_sni = cfg.server.effective_tls_sni();
                let sni = cli
                    .sni
                    .as_deref()
                    .or(cfg_sni.as_deref())
                    .unwrap_or(host.as_str());
                let cfg_ca = cfg.server.effective_tls_ca_file();
                let ca_bytes = match cli.ca_file.as_deref().or(cfg_ca.as_deref()) {
                    Some(p) => std::fs::read(p).ok(),
                    None => None,
                };
                handshake_tls(
                    &host,
                    port,
                    timeout_ms,
                    Some(sni),
                    auth_key.as_deref().map(str::as_bytes),
                    ca_bytes.as_deref(),
                )
                .await
            } else {
                handshake_tcp(
                    &host,
                    port,
                    timeout_ms,
                    auth_key.as_deref().map(str::as_bytes),
                )
                .await
            };
            match res {
                Ok(()) => println!("client_handshake.ok {host}:{port}"),
                Err(e) => {
                    eprintln!("client_handshake.fail {host}:{port} - {e}");
                    std::process::exit(5);
                }
            }
        }
        Commands::Client { bind, port, start, auth_token, open, static_dir, allow_external, mjpeg_width, mjpeg_height, mjpeg_fps, mjpeg_quality, allowed_origins, post_burst, post_refill, stream_burst, stream_refill, consent_allowed, ctrl_addr, ctrl_token, ctrl_ping_secs, ctrl_backoff_base_ms, ctrl_backoff_max_ms, ctrl_tls, ctrl_sni, ctrl_ca_file, ctrl_tofu_pin_file } => {
            // Initialize headless client and start HTTP control server
            let client = client_core::Client::new();
            let bind_addr = SocketAddr::new(bind.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST)), port);
            // Guard: prevent accidental external exposure unless explicitly allowed
            match bind_addr.ip() {
                IpAddr::V4(ip) if !ip.is_loopback() && !allow_external => {
                    eprintln!(
                        "refusing to bind to {}; pass --allow-external to override (ensure auth-token is set)",
                        bind_addr.ip()
                    );
                    std::process::exit(9);
                }
                IpAddr::V6(ip) if !ip.is_loopback() && !allow_external => {
                    eprintln!(
                        "refusing to bind to {}; pass --allow-external to override (ensure auth-token is set)",
                        bind_addr.ip()
                    );
                    std::process::exit(9);
                }
                _ => {}
            }
            // If explicitly allowing external bind, require an auth token for safety
            if allow_external && auth_token.is_none() {
                eprintln!(
                    "refusing to bind externally without --auth-token; set a token or remove --allow-external"
                );
                std::process::exit(9);
            }
            #[cfg(feature = "http-ui")]
            {
                if start {
                    let _ = client.start().await;
                }
                // Start QUIC control channel if configured (let client core own it, with TLS trust state)
                if let (Some(addr_s), Some(tok)) = (ctrl_addr.as_deref(), ctrl_token.as_deref()) {
                    let addr: SocketAddr = addr_s.parse().unwrap_or_else(|_| SocketAddr::from(([127,0,0,1], 4433)));
                    // Determine TLS mode
                    use transport::quic_ctrl::TlsMode;
                    let tls_mode = match ctrl_tls.as_deref().unwrap_or("insecure") {
                        "insecure" => TlsMode::InsecureNoVerify,
                        "system" => {
                            let sni = ctrl_sni.clone().unwrap_or_else(|| "localhost".into());
                            let ca_pem = match ctrl_ca_file.as_ref() { Some(p) => std::fs::read(p).ok(), None => None };
                            TlsMode::SystemRoots { sni, ca_pem }
                        }
                        m if m.starts_with("pin:") => {
                            let sni = ctrl_sni.clone().unwrap_or_else(|| "localhost".into());
                            let pin = m.trim_start_matches("pin:").to_string();
                            TlsMode::PinSha256 { sni, der_sha256_hex: pin }
                        }
                        "tofu" => {
                            let sni = ctrl_sni.clone().unwrap_or_else(|| "localhost".into());
                            let pin_file = ctrl_tofu_pin_file.clone();
                            let on_first = std::sync::Arc::new(move |pin: String| {
                                if let Some(ref p) = pin_file {
                                    let _ = std::fs::write(p, &pin);
                                }
                                eprintln!("ctrl.tofu.pin={}", pin);
                            });
                            TlsMode::Tofu { sni, on_first }
                        }
                        other => { eprintln!("unknown --ctrl-tls mode: {} (use insecure|system|pin:<hex>|tofu)", other); TlsMode::InsecureNoVerify }
                    };
                    // Load cached pin or CA
                    let cached_pin = ctrl_tofu_pin_file.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string());
                    let cached_ca = ctrl_ca_file.as_ref().and_then(|p| std::fs::read(p).ok());
                    // Delegate to client core to start and manage ctrl channel and status
                    let res = client.start_with_ctrl_tls_tuned(
                        addr,
                        tok.to_string(),
                        ctrl_ping_secs,
                        ctrl_backoff_base_ms,
                        ctrl_backoff_max_ms,
                        tls_mode,
                        cached_pin,
                        cached_ca,
                    ).await;
                    if let Err(e) = res {
                        eprintln!("ctrl.start.error: {}", e);
                    }
                }
                let defaults = client_http::StreamConfig {
                    default_width: mjpeg_width,
                    default_height: mjpeg_height,
                    default_fps: mjpeg_fps,
                    default_quality: mjpeg_quality,
                };
                let allow = allowed_origins.as_ref().map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>());
                let handle = client_http::serve(bind_addr, client, auth_token.clone(), static_dir, Some(defaults), allow, Some((post_burst, post_refill)), Some((stream_burst, stream_refill)), Some(consent_allowed)).await?;
                println!("client_http.started addr={}", handle.addr);
                if open {
                    let url = if let Some(tok) = auth_token.as_ref() {
                        format!("http://{}/#token={}", handle.addr, urlencoding::encode(tok))
                    } else {
                        format!("http://{}/", handle.addr)
                    };
                    let _ = webbrowser::open(&url);
                }
                // Wait for Ctrl+C or SIGTERM (Unix)
                #[cfg(unix)]
                {
                    use tokio::signal::unix::{signal, SignalKind};
                    let mut term = signal(SignalKind::terminate())?;
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {}
                        _ = term.recv() => {}
                    }
                }
                #[cfg(not(unix))]
                {
                    tokio::signal::ctrl_c().await?;
                }
                println!("client_http.stopping ...");
                handle.shutdown().await;
            }
            #[cfg(not(feature = "http-ui"))]
            {
                let _ = (bind_addr, start, auth_token, open, static_dir, allow_external, mjpeg_width, mjpeg_height, mjpeg_fps, mjpeg_quality, client, allowed_origins, post_burst, post_refill, stream_burst, stream_refill, consent_allowed, ctrl_addr, ctrl_token, ctrl_ping_secs, ctrl_backoff_base_ms, ctrl_backoff_max_ms, ctrl_tls, ctrl_sni, ctrl_ca_file, ctrl_tofu_pin_file);
                eprintln!("this build lacks 'http-ui' feature; rebuild CLI enabling client/http-ui");
                std::process::exit(7);
            }
        }
        Commands::CtrlServer { bind, token } => {
            // Start QUIC control server and simple stdin REPL for commands
            let bind = bind.unwrap_or_else(|| SocketAddr::from(([127,0,0,1], 4433)));
            let (addr, _join, tx_cmd, tx_sig) = quic_ctrl::start_ctrl_server(bind, token).await?;
            println!("ctrl_server.started addr={}", addr);
            println!("commands: start | stop | reauth_request | reauth <token> | quit");
            loop {
                // Read a line from stdin in a blocking task to avoid depending on tokio's io-std feature
                let read = tokio::task::spawn_blocking(|| -> std::io::Result<Option<String>> {
                    let mut s = String::new();
                    // Use std::io::stdin blocking read_line
                    let n = std::io::stdin().read_line(&mut s)?;
                    if n == 0 { Ok(None) } else { Ok(Some(s)) }
                });
                let line = tokio::select! {
                    res = read => res.unwrap_or(Ok(None)).map_err(|e| anyhow::anyhow!(e))?,
                    _ = tokio::signal::ctrl_c() => Some("quit".to_string()),
                };
                let Some(cmdline) = line else { break; };
                let parts: Vec<&str> = cmdline.trim().split_whitespace().collect();
                if parts.is_empty() { continue; }
                match parts[0] {
                    "start" => { let _ = tx_cmd.send(CtrlCmd::Start); println!("sent: start"); }
                    "stop" => { let _ = tx_cmd.send(CtrlCmd::Stop); println!("sent: stop"); }
                    "reauth_request" => { let _ = tx_sig.send(ServerSignal::ReauthRequest); println!("sent: reauth_request"); }
                    "reauth" => {
                        if parts.len() < 2 { println!("usage: reauth <token>"); continue; }
                        let new_tok = parts[1].to_string();
                        let _ = tx_sig.send(ServerSignal::UpdateToken(new_tok));
                        println!("updated expected token and requested client reauth");
                    }
                    "quit" | "exit" => { break; }
                    _ => println!("unknown command"),
                }
            }
        }
    }

    Ok(())
}
