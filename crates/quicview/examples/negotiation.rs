//! Negotiation example — simulates host / viewer handshake.

use quicview::protocol::*;
use quicview::session::*;

fn main() {
    quicview::init_tracing();

    // Host creates an offer
    let offer = SessionOffer {
        host_name: "my-workstation".into(),
        displays: vec![
            DisplayInfo {
                id: DisplayId(0),
                name: "DP-1".into(),
                resolution: Resolution::new(3840, 2160),
                refresh_hz: 60,
                is_virtual: false,
            },
            DisplayInfo {
                id: DisplayId(1),
                name: "HDMI-1".into(),
                resolution: Resolution::new(1920, 1080),
                refresh_hz: 144,
                is_virtual: false,
            },
        ],
        layout: DisplayLayout { entries: vec![] },
        max_fps: 60,
        supports_virtual_display: true,
    };

    println!("Host offer: {} ({} displays)", offer.host_name, offer.displays.len());

    // Viewer receives offer and selects display 0 at 1080p
    let mut neg = Negotiator::new();
    neg.receive_offer(offer).unwrap();

    let selection = vec![Negotiator::full_resolution(
        DisplayId(0),
        Resolution::new(1920, 1080),
    )];
    neg.select_displays(selection).unwrap();

    // Host accepts
    let accepted = neg.accept().unwrap();
    println!("Negotiation accepted: {} display(s) selected", accepted.len());
    for sel in &accepted {
        println!(
            "  Display {:?} at {}x{} @ {}Hz",
            sel.display_id,
            sel.resolution.width,
            sel.resolution.height,
            sel.refresh_hz,
        );
    }
}
