use serde::{Deserialize, Serialize};

use crate::display::{DisplayId, DisplayLayout, Resolution};

/// Control-plane message exchanged during session setup and runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Initial offer from host → viewer with available displays.
    Offer(SessionOffer),
    /// Viewer requests a specific display configuration.
    Negotiate(NegotiateDisplay),
    /// Host confirms the negotiated display is active.
    Accepted { display_id: DisplayId },
    /// Request a key frame (full refresh).
    RequestKeyFrame { display_id: DisplayId },
    /// Ping / keep-alive.
    Ping { timestamp_ms: u64 },
    /// Pong response.
    Pong { timestamp_ms: u64 },
    /// Graceful disconnect.
    Bye,
}

/// Host advertises its available displays (physical + virtual).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOffer {
    /// Host's human-readable name.
    pub host_name: String,
    /// Available displays on the host.
    pub displays: Vec<crate::display::DisplayInfo>,
    /// Current spatial layout.
    pub layout: DisplayLayout,
    /// Maximum frames per second the host can produce.
    pub max_fps: u32,
    /// Whether the host supports virtual display creation.
    pub supports_virtual_display: bool,
}

/// Viewer requests a display to stream (or creation of a virtual one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NegotiateDisplay {
    /// Which display to stream. `None` = create a new virtual display.
    pub display_id: Option<DisplayId>,
    /// Requested resolution (for virtual displays or scaling).
    pub resolution: Resolution,
    /// Requested refresh rate in Hz.
    pub refresh_hz: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::DisplayInfo;

    #[test]
    fn control_message_roundtrip_json() {
        let msg = ControlMessage::Offer(SessionOffer {
            host_name: "devbox".to_string(),
            displays: vec![DisplayInfo {
                id: DisplayId::PRIMARY,
                name: "HDMI-1".to_string(),
                resolution: Resolution::new(2560, 1440),
                refresh_hz: 144,
                is_virtual: false,
            }],
            layout: DisplayLayout {
                entries: vec![crate::display::DisplayEntry {
                    display_id: DisplayId::PRIMARY,
                    x: 0,
                    y: 0,
                    resolution: Resolution::new(2560, 1440),
                }],
            },
            max_fps: 60,
            supports_virtual_display: true,
        });

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: ControlMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, ControlMessage::Offer(_)));
    }
}
