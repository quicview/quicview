//! # quicview-capture
//!
//! Screen capture and virtual monitor creation for QuicView.
//!
//! Provides platform-agnostic traits for capturing frames from a display
//! and for creating software-backed virtual monitors that remote clients
//! can treat as extended displays.

pub mod error;
pub mod source;
pub mod virtual_display;

pub use error::CaptureError;
pub use source::{CaptureSource, FrameBuffer, TestCaptureSource};
pub use virtual_display::{StubVirtualDisplay, VirtualDisplay};
