pub mod config;
pub mod error;
pub mod ffi;
pub mod metrics;
pub mod observability;
pub mod shutdown;

// ── Re-exports from sub-crates ──────────────────────────────────────────────

pub use quicview_capture as capture;
pub use quicview_codec as codec;
pub use quicview_display as display;
pub use quicview_input as input;
pub use quicview_protocol as protocol;
pub use quicview_session as session;
pub use quicview_transport as transport;

pub use config::Config;
pub use error::QuicViewError;
pub use metrics::Metrics;
pub use observability::init_tracing;
pub use shutdown::{ShutdownController, ShutdownSignal};

/// Library version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
