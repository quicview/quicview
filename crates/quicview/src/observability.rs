use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Initialise the global tracing subscriber.
///
/// Reads the `RUST_LOG` environment variable. Falls back to `info` level
/// for QuicView crates if the variable is unset.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("quicview=info,quicview_protocol=info,quicview_codec=info,quicview_capture=info,quicview_display=info,quicview_input=info,quicview_session=info")
    });

    tracing_subscriber::registry()
        .with(fmt::layer().compact())
        .with(filter)
        .init();
}
