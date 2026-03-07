//! Basic QuicView example — enumerate test displays and grab a frame.

fn main() {
    quicview::init_tracing();

    // Create a test capture source
    let mut source = quicview::capture::TestCaptureSource::new(
        quicview::protocol::Resolution::new(1920, 1080),
    );

    // Enumerate
    let displays = quicview::capture::CaptureSource::enumerate_displays(&source).unwrap();
    println!("Found {} display(s):", displays.len());
    for d in &displays {
        println!(
            "  Display {} ({}) — {}x{} @ {}Hz (virtual: {})",
            d.id.0, d.name, d.resolution.width, d.resolution.height, d.refresh_hz, d.is_virtual,
        );
    }

    // Start capture
    quicview::capture::CaptureSource::start(&mut source, quicview::protocol::DisplayId::PRIMARY).unwrap();

    // Grab one frame
    let frame = quicview::capture::CaptureSource::grab(&mut source).unwrap().unwrap();
    println!(
        "Grabbed frame: {}x{} {:?} ({} bytes)",
        frame.resolution.width,
        frame.resolution.height,
        frame.pixel_format,
        frame.data.len(),
    );

    // Stop
    quicview::capture::CaptureSource::stop(&mut source).unwrap();
    println!("Done.");
}
