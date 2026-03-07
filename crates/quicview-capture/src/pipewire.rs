use quicview_protocol::{DisplayId, DisplayInfo};

use crate::error::CaptureError;
use crate::source::{CaptureSource, FrameBuffer};

/// PipeWire-based screen capture for Linux.
///
/// Not yet implemented. Returns [`CaptureError::PlatformNotSupported`].
pub struct PipeWireCaptureSource;

impl PipeWireCaptureSource {
    pub fn new() -> Result<Self, CaptureError> {
        Err(CaptureError::PlatformNotSupported(
            "PipeWire capture not yet implemented".into(),
        ))
    }
}

impl CaptureSource for PipeWireCaptureSource {
    fn enumerate_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Err(CaptureError::PlatformNotSupported("Linux".into()))
    }

    fn start(&mut self, _display_id: DisplayId) -> Result<(), CaptureError> {
        Err(CaptureError::PlatformNotSupported("Linux".into()))
    }

    fn grab(&mut self) -> Result<Option<FrameBuffer>, CaptureError> {
        Err(CaptureError::PlatformNotSupported("Linux".into()))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::PlatformNotSupported("Linux".into()))
    }
}
