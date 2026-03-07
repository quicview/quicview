use std::collections::HashMap;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::error::CaptureError;
use crate::virtual_display::{StubVirtualDisplay, VirtualDisplay};

use quicview_protocol::{DisplayId, DisplayLayout, DisplayEntry, Resolution};

/// Manages a wall of virtual displays across multiple headless clients.
///
/// Each client registers itself with a position in the grid. The manager
/// tracks which virtual display is assigned to which client and produces
/// the composite [`DisplayLayout`].
pub struct DisplayWall {
    clients: HashMap<SocketAddr, WallClient>,
    virtual_displays: StubVirtualDisplay,
}

/// A single client in the display wall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallClient {
    pub addr: SocketAddr,
    pub display_id: DisplayId,
    pub grid_x: i32,
    pub grid_y: i32,
    pub resolution: Resolution,
}

impl DisplayWall {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            virtual_displays: StubVirtualDisplay::new(),
        }
    }

    /// Register a new client at the given grid position.
    pub fn add_client(
        &mut self,
        addr: SocketAddr,
        grid_x: i32,
        grid_y: i32,
        resolution: Resolution,
    ) -> Result<DisplayId, CaptureError> {
        let display_id = self.virtual_displays.create(resolution, 60)?;
        let client = WallClient {
            addr,
            display_id,
            grid_x,
            grid_y,
            resolution,
        };
        self.clients.insert(addr, client);
        Ok(display_id)
    }

    /// Remove a client and destroy its virtual display.
    pub fn remove_client(&mut self, addr: &SocketAddr) -> Result<(), CaptureError> {
        if let Some(client) = self.clients.remove(addr) {
            self.virtual_displays.destroy(client.display_id)?;
        }
        Ok(())
    }

    /// Build the composite layout from all registered clients.
    pub fn layout(&self) -> DisplayLayout {
        let entries: Vec<DisplayEntry> = self
            .clients
            .values()
            .map(|c| DisplayEntry {
                display_id: c.display_id,
                x: c.grid_x * c.resolution.width as i32,
                y: c.grid_y * c.resolution.height as i32,
                resolution: c.resolution,
            })
            .collect();
        DisplayLayout { entries }
    }

    /// Number of connected clients.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Get a client by address.
    pub fn get_client(&self, addr: &SocketAddr) -> Option<&WallClient> {
        self.clients.get(addr)
    }
}

impl Default for DisplayWall {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_wall_lifecycle() {
        let mut wall = DisplayWall::new();
        let addr1: SocketAddr = "192.168.1.10:4433".parse().unwrap();
        let addr2: SocketAddr = "192.168.1.11:4433".parse().unwrap();

        let id1 = wall.add_client(addr1, 0, 0, Resolution::new(1920, 1080)).unwrap();
        let id2 = wall.add_client(addr2, 1, 0, Resolution::new(1920, 1080)).unwrap();
        assert_ne!(id1, id2);

        assert_eq!(wall.client_count(), 2);
        let layout = wall.layout();
        assert_eq!(layout.entries.len(), 2);

        // Bounding box should be 3840x1080 (two 1920-wide monitors side by side).
        let (_, _, w, h) = layout.bounding_box();
        assert_eq!(w, 3840);
        assert_eq!(h, 1080);

        wall.remove_client(&addr1).unwrap();
        assert_eq!(wall.client_count(), 1);
    }
}
