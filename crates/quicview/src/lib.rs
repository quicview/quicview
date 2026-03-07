pub mod error;
pub mod ffi;
pub mod observability;

// ── Re-exports from sub-crates ──────────────────────────────────────────────

pub use quicview_capture as capture;
pub use quicview_codec as codec;
pub use quicview_display as display;
pub use quicview_input as input;
pub use quicview_protocol as protocol;
pub use quicview_session as session;

pub use error::QuicViewError;
pub use observability::init_tracing;

/// Library version (from Cargo.toml).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
