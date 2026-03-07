use quicview_protocol::{DisplayId, Resolution};

use crate::error::CaptureError;

/// Trait for creating and managing virtual (software) displays.
///
/// Virtual displays allow a remote client to "extend" the host's desktop
/// with additional monitors that only exist over the network — the
/// infinite-display-wall scenario.
pub trait VirtualDisplay: Send {
    /// Create a new virtual display with the given resolution and refresh rate.
    /// Returns the assigned [`DisplayId`].
    fn create(
        &mut self,
        resolution: Resolution,
        refresh_hz: u32,
    ) -> Result<DisplayId, CaptureError>;

    /// Resize an existing virtual display.
    fn resize(
        &mut self,
        display_id: DisplayId,
        resolution: Resolution,
    ) -> Result<(), CaptureError>;

    /// Destroy a virtual display.
    fn destroy(&mut self, display_id: DisplayId) -> Result<(), CaptureError>;

    /// List all virtual display IDs currently managed.
    fn list(&self) -> Vec<DisplayId>;
}

/// Stub implementation that tracks virtual displays in memory without
/// creating real OS-level monitors. Useful for testing and headless mode.
pub struct StubVirtualDisplay {
    next_id: u32,
    displays: Vec<(DisplayId, Resolution, u32)>,
}

impl StubVirtualDisplay {
    pub fn new() -> Self {
        Self {
            next_id: 100, // start virtual IDs above typical physical IDs
            displays: Vec::new(),
        }
    }
}

impl Default for StubVirtualDisplay {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualDisplay for StubVirtualDisplay {
    fn create(
        &mut self,
        resolution: Resolution,
        refresh_hz: u32,
    ) -> Result<DisplayId, CaptureError> {
        let id = DisplayId(self.next_id);
        self.next_id += 1;
        self.displays.push((id, resolution, refresh_hz));
        Ok(id)
    }

    fn resize(
        &mut self,
        display_id: DisplayId,
        resolution: Resolution,
    ) -> Result<(), CaptureError> {
        for entry in &mut self.displays {
            if entry.0 == display_id {
                entry.1 = resolution;
                return Ok(());
            }
        }
        Err(CaptureError::DisplayNotFound(display_id.0))
    }

    fn destroy(&mut self, display_id: DisplayId) -> Result<(), CaptureError> {
        let before = self.displays.len();
        self.displays.retain(|e| e.0 != display_id);
        if self.displays.len() == before {
            Err(CaptureError::DisplayNotFound(display_id.0))
        } else {
            Ok(())
        }
    }

    fn list(&self) -> Vec<DisplayId> {
        self.displays.iter().map(|e| e.0).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_virtual_display_lifecycle() {
        let mut vd = StubVirtualDisplay::new();

        let id = vd.create(Resolution::new(1920, 1080), 60).unwrap();
        assert_eq!(vd.list().len(), 1);

        vd.resize(id, Resolution::new(2560, 1440)).unwrap();

        let id2 = vd.create(Resolution::new(1280, 720), 30).unwrap();
        assert_eq!(vd.list().len(), 2);
        assert_ne!(id, id2);

        vd.destroy(id).unwrap();
        assert_eq!(vd.list().len(), 1);
        assert_eq!(vd.list()[0], id2);
    }
}
