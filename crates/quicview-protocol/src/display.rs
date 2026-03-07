use serde::{Deserialize, Serialize};

/// Unique identifier for a display (physical or virtual).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayId(pub u32);

impl DisplayId {
    /// The primary (default) display.
    pub const PRIMARY: Self = Self(0);
}

impl std::fmt::Display for DisplayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "display:{}", self.0)
    }
}

/// Screen resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

impl Resolution {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Total number of pixels.
    pub const fn pixel_count(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

impl std::fmt::Display for Resolution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// Pixel format of a captured or rendered frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    /// 32-bit BGRA (common on Windows / macOS screen capture).
    Bgra8,
    /// 32-bit RGBA.
    Rgba8,
    /// 24-bit RGB (no alpha).
    Rgb8,
    /// NV12 planar (Y + interleaved UV), common for hardware encoders.
    Nv12,
}

impl PixelFormat {
    /// Bytes per pixel (for packed formats). Returns `None` for planar formats.
    pub const fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            Self::Bgra8 | Self::Rgba8 => Some(4),
            Self::Rgb8 => Some(3),
            Self::Nv12 => None, // planar
        }
    }

    /// Compute buffer size for a given resolution.
    pub const fn buffer_size(&self, res: Resolution) -> usize {
        let pixels = res.pixel_count() as usize;
        match self {
            Self::Bgra8 | Self::Rgba8 => pixels * 4,
            Self::Rgb8 => pixels * 3,
            Self::Nv12 => pixels + pixels / 2, // Y plane + UV plane
        }
    }
}

/// Metadata describing a single display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayInfo {
    /// Display identifier.
    pub id: DisplayId,
    /// Human-readable name (e.g. "HDMI-1", "Virtual-3").
    pub name: String,
    /// Native resolution.
    pub resolution: Resolution,
    /// Refresh rate in Hz.
    pub refresh_hz: u32,
    /// Whether this is a virtual (software-created) display.
    pub is_virtual: bool,
}

/// Spatial arrangement of multiple displays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayLayout {
    /// Ordered list of displays with their positions.
    pub entries: Vec<DisplayEntry>,
}

/// A display positioned in the layout grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayEntry {
    pub display_id: DisplayId,
    /// X offset in the virtual desktop (pixels from left).
    pub x: i32,
    /// Y offset in the virtual desktop (pixels from top).
    pub y: i32,
    pub resolution: Resolution,
}

impl DisplayLayout {
    /// Bounding box of the entire layout.
    pub fn bounding_box(&self) -> (i32, i32, u32, u32) {
        if self.entries.is_empty() {
            return (0, 0, 0, 0);
        }
        let min_x = self.entries.iter().map(|e| e.x).min().unwrap_or(0);
        let min_y = self.entries.iter().map(|e| e.y).min().unwrap_or(0);
        let max_x = self
            .entries
            .iter()
            .map(|e| e.x + e.resolution.width as i32)
            .max()
            .unwrap_or(0);
        let max_y = self
            .entries
            .iter()
            .map(|e| e.y + e.resolution.height as i32)
            .max()
            .unwrap_or(0);
        (min_x, min_y, (max_x - min_x) as u32, (max_y - min_y) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_pixel_count() {
        let r = Resolution::new(1920, 1080);
        assert_eq!(r.pixel_count(), 2_073_600);
    }

    #[test]
    fn pixel_format_buffer_sizes() {
        let hd = Resolution::new(1920, 1080);
        assert_eq!(PixelFormat::Bgra8.buffer_size(hd), 1920 * 1080 * 4);
        assert_eq!(PixelFormat::Rgb8.buffer_size(hd), 1920 * 1080 * 3);
        assert_eq!(PixelFormat::Nv12.buffer_size(hd), 1920 * 1080 * 3 / 2);
    }

    #[test]
    fn display_layout_bounding_box() {
        let layout = DisplayLayout {
            entries: vec![
                DisplayEntry {
                    display_id: DisplayId(0),
                    x: 0,
                    y: 0,
                    resolution: Resolution::new(1920, 1080),
                },
                DisplayEntry {
                    display_id: DisplayId(1),
                    x: 1920,
                    y: 0,
                    resolution: Resolution::new(1920, 1080),
                },
            ],
        };
        assert_eq!(layout.bounding_box(), (0, 0, 3840, 1080));
    }

    #[test]
    fn display_id_formatting() {
        assert_eq!(DisplayId::PRIMARY.to_string(), "display:0");
        assert_eq!(DisplayId(3).to_string(), "display:3");
    }
}
