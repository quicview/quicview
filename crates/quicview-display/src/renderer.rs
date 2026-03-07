use bytes::Bytes;
use quicview_protocol::{DisplayId, FrameHeader, PixelFormat, Resolution};

use crate::error::DisplayError;

/// Trait for consuming decoded frames and presenting them to the user.
pub trait FrameRenderer: Send {
    /// Present a decoded frame.
    ///
    /// `header` describes the frame metadata; `pixels` contains raw pixel
    /// data in the format indicated by the header.
    fn render(
        &mut self,
        header: &FrameHeader,
        pixels: &[u8],
    ) -> Result<(), DisplayError>;

    /// Resize the rendering target (e.g. window was resized).
    fn resize(&mut self, resolution: Resolution) -> Result<(), DisplayError>;
}

/// A renderer that simply logs frame metadata via [`tracing`]. Useful for
/// headless benchmarks and integration tests.
pub struct LogRenderer {
    frames_rendered: u64,
}

impl LogRenderer {
    pub fn new() -> Self {
        Self { frames_rendered: 0 }
    }

    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered
    }
}

impl Default for LogRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameRenderer for LogRenderer {
    fn render(
        &mut self,
        header: &FrameHeader,
        _pixels: &[u8],
    ) -> Result<(), DisplayError> {
        tracing::debug!(
            display = header.display_id.0,
            seq = header.sequence,
            w = header.resolution.width,
            h = header.resolution.height,
            "rendered frame"
        );
        self.frames_rendered += 1;
        Ok(())
    }

    fn resize(&mut self, resolution: Resolution) -> Result<(), DisplayError> {
        tracing::debug!(w = resolution.width, h = resolution.height, "resize");
        Ok(())
    }
}

/// A renderer that stores frames into an in-memory buffer for testing.
pub struct BufferRenderer {
    display_id: DisplayId,
    last_frame: Option<Bytes>,
    format: PixelFormat,
    frames_rendered: u64,
}

impl BufferRenderer {
    pub fn new(display_id: DisplayId, format: PixelFormat) -> Self {
        Self {
            display_id,
            last_frame: None,
            format,
            frames_rendered: 0,
        }
    }

    pub fn last_frame(&self) -> Option<&Bytes> {
        self.last_frame.as_ref()
    }

    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered
    }

    pub fn format(&self) -> PixelFormat {
        self.format
    }

    pub fn display_id(&self) -> DisplayId {
        self.display_id
    }
}

impl FrameRenderer for BufferRenderer {
    fn render(
        &mut self,
        header: &FrameHeader,
        pixels: &[u8],
    ) -> Result<(), DisplayError> {
        if header.display_id != self.display_id {
            return Err(DisplayError::DisplayNotFound(header.display_id.0));
        }
        self.last_frame = Some(Bytes::copy_from_slice(pixels));
        self.frames_rendered += 1;
        Ok(())
    }

    fn resize(&mut self, _resolution: Resolution) -> Result<(), DisplayError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quicview_protocol::FrameKind;

    fn test_header() -> FrameHeader {
        FrameHeader {
            kind: FrameKind::Full,
            display_id: DisplayId(1),
            sequence: 42,
            resolution: Resolution::new(2, 2),
            pixel_format: PixelFormat::Rgba8,
            payload_len: 16,
        }
    }

    #[test]
    fn log_renderer_counts() {
        let mut r = LogRenderer::new();
        let h = test_header();
        let pixels = vec![0u8; 16];
        r.render(&h, &pixels).unwrap();
        r.render(&h, &pixels).unwrap();
        assert_eq!(r.frames_rendered(), 2);
    }

    #[test]
    fn buffer_renderer_stores_frame() {
        let mut r = BufferRenderer::new(DisplayId(1), PixelFormat::Rgba8);
        let h = test_header();
        let pixels = vec![0xAB; 16];
        r.render(&h, &pixels).unwrap();
        assert_eq!(r.last_frame().unwrap().as_ref(), &[0xAB; 16]);
        assert_eq!(r.frames_rendered(), 1);
    }

    #[test]
    fn buffer_renderer_rejects_wrong_display() {
        let mut r = BufferRenderer::new(DisplayId(99), PixelFormat::Rgba8);
        let h = test_header(); // display_id = 1
        let pixels = vec![0u8; 16];
        assert!(r.render(&h, &pixels).is_err());
    }
}
