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
pub mod wall;

// Platform-specific capture implementations.
#[cfg(target_os = "windows")]
pub mod gdi;
#[cfg(target_os = "linux")]
pub mod pipewire;
#[cfg(target_os = "macos")]
pub mod screencapturekit;

pub use error::CaptureError;
pub use source::{CaptureSource, FrameBuffer, TestCaptureSource};
pub use virtual_display::{StubVirtualDisplay, VirtualDisplay};
pub use wall::DisplayWall;

// Platform capture source re-exports.
#[cfg(target_os = "windows")]
pub use gdi::GdiCaptureSource;
#[cfg(target_os = "linux")]
pub use pipewire::PipeWireCaptureSource;
#[cfg(target_os = "macos")]
pub use screencapturekit::ScreenCaptureKitSource;
