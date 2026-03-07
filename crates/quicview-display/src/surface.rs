use quicview_protocol::{DisplayId, Resolution};

use crate::error::DisplayError;

/// A display surface represents an output target for rendered frames.
///
/// On a real system this wraps a native window or GPU texture. For testing
/// and headless scenarios, [`MemorySurface`] stores pixel data in RAM.
pub trait DisplaySurface: Send {
    /// The display this surface is bound to.
    fn display_id(&self) -> DisplayId;

    /// Current resolution of the surface.
    fn resolution(&self) -> Resolution;

    /// Resize the surface.
    fn resize(&mut self, resolution: Resolution) -> Result<(), DisplayError>;

    /// Write a raw pixel buffer to this surface.
    fn present(&mut self, pixels: &[u8]) -> Result<(), DisplayError>;
}

/// In-memory surface for testing. Stores the latest pixel buffer.
pub struct MemorySurface {
    display_id: DisplayId,
    resolution: Resolution,
    buffer: Vec<u8>,
}

impl MemorySurface {
    pub fn new(display_id: DisplayId, resolution: Resolution) -> Self {
        let buf_len = (resolution.width * resolution.height * 4) as usize;
        Self {
            display_id,
            resolution,
            buffer: vec![0u8; buf_len],
        }
    }

    /// Read back the current pixel buffer.
    pub fn pixels(&self) -> &[u8] {
        &self.buffer
    }
}

impl DisplaySurface for MemorySurface {
    fn display_id(&self) -> DisplayId {
        self.display_id
    }

    fn resolution(&self) -> Resolution {
        self.resolution
    }

    fn resize(&mut self, resolution: Resolution) -> Result<(), DisplayError> {
        self.resolution = resolution;
        let buf_len = (resolution.width * resolution.height * 4) as usize;
        self.buffer.resize(buf_len, 0);
        Ok(())
    }

    fn present(&mut self, pixels: &[u8]) -> Result<(), DisplayError> {
        let expected = (self.resolution.width * self.resolution.height * 4) as usize;
        if pixels.len() != expected {
            return Err(DisplayError::RenderFailed(format!(
                "buffer size mismatch: got {} expected {}",
                pixels.len(),
                expected
            )));
        }
        self.buffer.copy_from_slice(pixels);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_surface_present_and_read() {
        let res = Resolution::new(2, 2);
        let mut s = MemorySurface::new(DisplayId(1), res);
        assert_eq!(s.pixels().len(), 16); // 2*2*4

        let data = vec![0xFF; 16];
        s.present(&data).unwrap();
        assert_eq!(s.pixels(), &[0xFF; 16]);
    }

    #[test]
    fn memory_surface_resize() {
        let mut s = MemorySurface::new(DisplayId(1), Resolution::new(2, 2));
        s.resize(Resolution::new(4, 4)).unwrap();
        assert_eq!(s.resolution().width, 4);
        assert_eq!(s.pixels().len(), 64); // 4*4*4
    }

    #[test]
    fn memory_surface_rejects_bad_buffer() {
        let mut s = MemorySurface::new(DisplayId(1), Resolution::new(2, 2));
        let data = vec![0u8; 8]; // too small
        assert!(s.present(&data).is_err());
    }
}
