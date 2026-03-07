//! Virtual display example — create and manage virtual monitors.

use quicview::capture::{VirtualDisplay, StubVirtualDisplay};
use quicview::protocol::{DisplayId, Resolution};

fn main() {
    quicview::init_tracing();

    let mut vd = StubVirtualDisplay::new();

    // Extend desktop with 3 virtual displays (simulates 3 RPis)
    let displays: Vec<DisplayId> = (0..3)
        .map(|i| {
            let res = Resolution::new(1920, 1080);
            let id = vd.create(res, 60).unwrap();
            println!("Created virtual display {} (1920x1080 @ 60Hz) for RPi-{}", id.0, i);
            id
        })
        .collect();

    println!("\n{} virtual displays active:", vd.list().len());

    // Resize one to 4K
    vd.resize(displays[0], Resolution::new(3840, 2160)).unwrap();
    println!("Resized display {} to 4K", displays[0].0);

    // Remove middle display
    vd.destroy(displays[1]).unwrap();
    println!("Destroyed display {}", displays[1].0);

    println!("\nRemaining: {:?}", vd.list().iter().map(|d| d.0).collect::<Vec<_>>());
}
