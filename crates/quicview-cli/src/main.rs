use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "quicview", version, about = "QUIC-native visual streaming runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Share displays from this machine (host role).
    Serve {
        /// Bind address.
        #[arg(short, long, default_value = "0.0.0.0:4433")]
        bind: String,
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
}

fn main() {
    quicview::init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Command::Serve { bind } => {
            tracing::info!(bind, "starting host — sharing displays");
            // TODO: start capture → encode → QUIC stream pipeline
            eprintln!("quicview serve is not yet implemented (bind={bind})");
        }
        Command::Connect { remote } => {
            tracing::info!(remote, "connecting as viewer");
            // TODO: QUIC connect → decode → render pipeline
            eprintln!("quicview connect is not yet implemented (remote={remote})");
        }
        Command::Extend { remote, resolution } => {
            tracing::info!(remote, resolution, "extending desktop");
            // TODO: create virtual display → capture → stream to remote
            eprintln!(
                "quicview extend is not yet implemented (remote={remote}, resolution={resolution})"
            );
        }
    }
}
