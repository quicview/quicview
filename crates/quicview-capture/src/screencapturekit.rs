use quicview_protocol::{DisplayId, DisplayInfo};

use crate::error::CaptureError;
use crate::source::{CaptureSource, FrameBuffer};

/// ScreenCaptureKit-based screen capture for macOS.
///
/// Not yet implemented. Returns [`CaptureError::PlatformNotSupported`].
pub struct ScreenCaptureKitSource;

impl ScreenCaptureKitSource {
    pub fn new() -> Result<Self, CaptureError> {
        Err(CaptureError::PlatformNotSupported(
            "ScreenCaptureKit capture not yet implemented".into(),
        ))
    }
}

impl CaptureSource for ScreenCaptureKitSource {
    fn enumerate_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Err(CaptureError::PlatformNotSupported("macOS".into()))
    }

    fn start(&mut self, _display_id: DisplayId) -> Result<(), CaptureError> {
        Err(CaptureError::PlatformNotSupported("macOS".into()))
    }

    fn grab(&mut self) -> Result<Option<FrameBuffer>, CaptureError> {
        Err(CaptureError::PlatformNotSupported("macOS".into()))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::PlatformNotSupported("macOS".into()))
    }
}
