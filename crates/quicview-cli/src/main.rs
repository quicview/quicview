use clap::{Parser, Subcommand};

use quicview_transport::{QuicConnection, QuicListener, SelfSignedCert, StreamKind, MAX_CONTROL_MESSAGE_SIZE};

#[derive(Parser)]
#[command(name = "quicview", version, about = "QUIC-native visual streaming runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Path to a TOML configuration file.
    #[arg(short, long, global = true)]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Share displays from this machine (host role).
    Serve {
        /// Bind address.
        #[arg(short, long)]
        bind: Option<String>,
    },
    /// Connect to a remote host as a viewer.
    Connect {
        /// Remote host address.
        #[arg(short, long)]
        remote: String,
    },
    /// Extend your desktop onto a remote device's virtual display.
    Extend {
        /// Remote device address.
        #[arg(short, long)]
        remote: String,
        /// Virtual display resolution (WxH).
        #[arg(long, default_value = "1920x1080")]
        resolution: String,
    },
    /// Print runtime metrics / status.
    Status,
    /// Generate a default configuration file.
    Init,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    quicview::init_tracing();
    let cli = Cli::parse();

    let config = match &cli.config {
        Some(path) => quicview::Config::load(std::path::Path::new(path))?,
        None => quicview::Config::default(),
    };

    let metrics = quicview::Metrics::new();

    match cli.command {
        Command::Serve { bind } => {
            let bind_addr = bind.unwrap_or_else(|| config.bind_addr());
            let addr = bind_addr.parse()?;
            let cert = SelfSignedCert::generate(&["localhost"])?;
            tracing::info!(fingerprint = %cert.fingerprint().to_hex(), "server certificate fingerprint");
            let listener = QuicListener::bind(addr, &cert)?;
            let local = listener.local_addr()?;
            tracing::info!(%local, "host listening — waiting for viewers");

            let (shutdown_ctrl, _shutdown_sig) = quicview::ShutdownController::new();

            // Spawn a task that listens for Ctrl+C and triggers shutdown.
            let ctrl = shutdown_ctrl.clone();
            tokio::spawn(async move {
                if tokio::signal::ctrl_c().await.is_ok() {
                    tracing::info!("received Ctrl+C — shutting down");
                    ctrl.trigger();
                }
            });

            let mut shutdown_sig = shutdown_ctrl.signal();
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        let conn = result?;
                        let remote = conn.remote_address();
                        tracing::info!(%remote, "viewer connected");
                        metrics.connection_opened();

                        let mux = QuicListener::mux(conn);
                        let m = metrics.clone();
                        let mut sig = shutdown_ctrl.signal();
                        tokio::spawn(async move {
                            tokio::select! {
                                result = handle_viewer(mux) => {
                                    if let Err(e) = result {
                                        tracing::error!(%remote, error = %e, "viewer session ended");
                                        m.record_error();
                                    }
                                }
                                () = sig.wait() => {
                                    tracing::info!(%remote, "shutdown — disconnecting viewer");
                                }
                            }
                            m.connection_closed();
                        });
                    }
                    () = shutdown_sig.wait() => {
                        tracing::info!("shutting down listener");
                        break;
                    }
                }
            }

            let snap = metrics.snapshot();
            tracing::info!(
                total_connections = snap.total_connections,
                total_errors = snap.errors,
                "server stopped"
            );
        }
        Command::Connect { remote } => {
            let addr = remote.parse()?;
            tracing::info!(%remote, "connecting as viewer");
            let conn = QuicConnection::connect(addr, "localhost").await?;
            tracing::info!(peer = %conn.remote_address(), "connected to host");

            let mux = conn.mux();
            let (mut send, mut recv) = mux.open(StreamKind::Control).await?;
            tracing::info!("control stream opened");

            // Send a Ping control message.
            let ping = quicview_protocol::ControlMessage::Ping {
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            };
            let payload = serde_json::to_vec(&ping)?;
            send.write_all(&(payload.len() as u32).to_be_bytes())
                .await?;
            send.write_all(&payload).await?;
            tracing::info!("sent ping");

            // Read response with size limit.
            let mut len_buf = [0u8; 4];
            recv.read_exact(&mut len_buf).await?;
            let len = u32::from_be_bytes(len_buf) as usize;
            if len > MAX_CONTROL_MESSAGE_SIZE {
                return Err(format!("control response too large: {len} bytes").into());
            }
            let mut buf = vec![0u8; len];
            recv.read_exact(&mut buf).await?;
            let msg: quicview_protocol::ControlMessage = serde_json::from_slice(&buf)?;
            tracing::info!(?msg, "received response");

            conn.close();
        }
        Command::Extend { remote, resolution } => {
            tracing::info!(remote, resolution, "extending desktop");
            // TODO: create virtual display → capture → stream to remote
            eprintln!(
                "quicview extend is not yet implemented (remote={remote}, resolution={resolution})"
            );
        }
        Command::Status => {
            println!("{}", metrics.snapshot());
            println!("\nQuicView v{}", quicview::VERSION);
        }
        Command::Init => {
            let path = std::path::Path::new("quicview.toml");
            if path.exists() {
                eprintln!("quicview.toml already exists — not overwriting");
            } else {
                let default_config = quicview::Config::default();
                let toml_str = default_config.to_toml()?;
                std::fs::write(path, toml_str)?;
                println!("wrote default configuration to quicview.toml");
            }
        }
    }

    Ok(())
}

/// Handle a single viewer session on the host side.
async fn handle_viewer(
    mux: quicview_transport::StreamMux,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (kind, mut send, mut recv) = mux.accept().await?;
    tracing::info!(?kind, "accepted stream");

    match kind {
        StreamKind::Control => {
            // Read length-prefixed control messages with size limit.
            let mut len_buf = [0u8; 4];
            recv.read_exact(&mut len_buf).await?;
            let len = u32::from_be_bytes(len_buf) as usize;
            if len > MAX_CONTROL_MESSAGE_SIZE {
                return Err(format!("control message too large: {len} bytes (max {MAX_CONTROL_MESSAGE_SIZE})").into());
            }
            let mut buf = vec![0u8; len];
            recv.read_exact(&mut buf).await?;

            let msg: quicview_protocol::ControlMessage = serde_json::from_slice(&buf)?;
            tracing::info!(?msg, "received control message");

            // Reply with Pong if Ping.
            if let quicview_protocol::ControlMessage::Ping { timestamp_ms } = msg {
                let pong = quicview_protocol::ControlMessage::Pong { timestamp_ms };
                let payload = serde_json::to_vec(&pong)?;
                send.write_all(&(payload.len() as u32).to_be_bytes())
                    .await?;
                send.write_all(&payload).await?;
                tracing::info!("sent pong");
            }
        }
        StreamKind::Video | StreamKind::Input => {
            tracing::warn!(?kind, "stream kind not yet handled on host");
        }
    }

    Ok(())
}
