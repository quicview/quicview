use quicview_protocol::{DisplayId, DisplayInfo, PixelFormat, Resolution};

use crate::error::CaptureError;

/// A captured frame's raw pixel buffer.
pub struct FrameBuffer {
    /// Raw pixel data in the format given by `pixel_format`.
    pub data: Vec<u8>,
    /// Pixel format of this buffer.
    pub pixel_format: PixelFormat,
    /// Resolution of this buffer.
    pub resolution: Resolution,
    /// Capture timestamp in microseconds since an arbitrary epoch.
    pub timestamp_us: u64,
}

/// Trait for screen capture implementations.
///
/// Each platform (Windows DXGI, macOS ScreenCaptureKit, Linux
/// PipeWire/X11) provides its own implementation.
pub trait CaptureSource: Send {
    /// List all available displays.
    fn enumerate_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError>;

    /// Start capturing the given display.
    fn start(&mut self, display_id: DisplayId) -> Result<(), CaptureError>;

    /// Grab the next frame. Returns `None` if no new frame is available
    /// (i.e. the screen hasn't changed since the last grab).
    fn grab(&mut self) -> Result<Option<FrameBuffer>, CaptureError>;

    /// Stop capturing.
    fn stop(&mut self) -> Result<(), CaptureError>;
}

/// A dummy capture source for testing that produces solid-color frames.
pub struct TestCaptureSource {
    active: bool,
    display_id: DisplayId,
    resolution: Resolution,
    frame_count: u64,
}

impl TestCaptureSource {
    pub fn new(resolution: Resolution) -> Self {
        Self {
            active: false,
            display_id: DisplayId::PRIMARY,
            resolution,
            frame_count: 0,
        }
    }
}

impl CaptureSource for TestCaptureSource {
    fn enumerate_displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        Ok(vec![DisplayInfo {
            id: DisplayId::PRIMARY,
            name: "Test Display".to_string(),
            resolution: self.resolution,
            refresh_hz: 60,
            is_virtual: false,
        }])
    }

    fn start(&mut self, display_id: DisplayId) -> Result<(), CaptureError> {
        self.display_id = display_id;
        self.active = true;
        Ok(())
    }

    fn grab(&mut self) -> Result<Option<FrameBuffer>, CaptureError> {
        if !self.active {
            return Err(CaptureError::CaptureFailed(
                "not started".to_string(),
            ));
        }
        self.frame_count += 1;
        let size = PixelFormat::Bgra8.buffer_size(self.resolution);
        // Produce a solid-color frame (color cycles with frame count).
        let color = (self.frame_count % 256) as u8;
        let data = vec![color; size];
        Ok(Some(FrameBuffer {
            data,
            pixel_format: PixelFormat::Bgra8,
            resolution: self.resolution,
            timestamp_us: self.frame_count * 16_667, // ~60 fps
        }))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.active = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_source_lifecycle() {
        let mut src = TestCaptureSource::new(Resolution::new(320, 240));

        let displays = src.enumerate_displays().unwrap();
        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].resolution.width, 320);

        src.start(DisplayId::PRIMARY).unwrap();

        let frame = src.grab().unwrap().unwrap();
        assert_eq!(frame.resolution, Resolution::new(320, 240));
        assert_eq!(frame.pixel_format, PixelFormat::Bgra8);
        assert_eq!(frame.data.len(), 320 * 240 * 4);

        src.stop().unwrap();
    }
}
