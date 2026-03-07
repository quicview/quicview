//! Integration tests for QuicView workspace.

use quicview::protocol::*;
use quicview::codec::{RawCodec, Encoder, Decoder};
use quicview::capture::{CaptureSource, TestCaptureSource};
use quicview::display::{FrameRenderer, LogRenderer};
use quicview::input::{InputInjector, LogInjector};
use quicview::session::*;

#[test]
fn full_pipeline_capture_encode_decode_render() {
    // 1. Capture a frame
    let mut source = TestCaptureSource::new(Resolution::new(4, 4));
    CaptureSource::start(&mut source, DisplayId::PRIMARY).unwrap();
    let frame = CaptureSource::grab(&mut source).unwrap().unwrap();

    // 2. Build a frame header
    let header = FrameHeader {
        kind: FrameKind::Full,
        display_id: DisplayId(0),
        sequence: 1,
        resolution: frame.resolution,
        pixel_format: frame.pixel_format,
        payload_len: frame.data.len() as u32,
    };

    // 3. Encode
    let mut codec = RawCodec;
    let encoded = codec.encode(&frame.data).unwrap();

    // 4. Decode
    let decoded = codec.decode(&encoded).unwrap();
    assert_eq!(decoded.len(), frame.data.len());

    // 5. Render
    let mut renderer = LogRenderer::new();
    renderer.render(&header, &decoded).unwrap();
    assert_eq!(renderer.frames_rendered(), 1);

    CaptureSource::stop(&mut source).unwrap();
}

#[test]
fn negotiation_and_auth_flow() {
    // Auth
    let validator = AcceptAll;
    let token = SessionToken::new(b"test-token".to_vec());
    validator.validate(&token).unwrap();

    // Negotiation
    let offer = SessionOffer {
        host_name: "test-host".into(),
        displays: vec![DisplayInfo {
            id: DisplayId(0),
            name: "Primary".into(),
            resolution: Resolution::new(1920, 1080),
            refresh_hz: 60,
            is_virtual: false,
        }],
        layout: DisplayLayout { entries: vec![] },
        max_fps: 60,
        supports_virtual_display: false,
    };

    let mut neg = Negotiator::new();
    neg.receive_offer(offer).unwrap();

    let selections = vec![Negotiator::full_resolution(
        DisplayId(0),
        Resolution::new(1920, 1080),
    )];
    neg.select_displays(selections).unwrap();

    let accepted = neg.accept().unwrap();
    assert_eq!(accepted.len(), 1);
}

#[test]
fn input_injection_pipeline() {
    let mut injector = LogInjector::new();

    let events = vec![
        InputEvent::Mouse(MouseEvent {
            x: 100,
            y: 200,
            button: Some((MouseButton::Left, KeyAction::Press)),
        }),
        InputEvent::Key(KeyEvent {
            keycode: 0x04,
            action: KeyAction::Press,
            modifiers: 0,
        }),
        InputEvent::Scroll(ScrollEvent { dx: 0.0, dy: -3.0 }),
    ];

    for event in &events {
        injector.inject(event).unwrap();
    }
    assert_eq!(injector.events_injected(), 3);
}
