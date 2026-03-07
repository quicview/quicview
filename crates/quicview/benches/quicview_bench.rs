use criterion::{Criterion, criterion_group, criterion_main};
use bytes::BytesMut;
use quicview::protocol::{FrameHeader, FrameKind, DisplayId, PixelFormat, Resolution};

fn bench_frame_header_roundtrip(c: &mut Criterion) {
    let header = FrameHeader {
        kind: FrameKind::Full,
        display_id: DisplayId(0),
        sequence: 12345,
        resolution: Resolution::new(1920, 1080),
        pixel_format: PixelFormat::Bgra8,
        payload_len: 1920 * 1080 * 4,
    };

    c.bench_function("frame_header_encode", |b| {
        b.iter(|| {
            let mut buf = BytesMut::with_capacity(24);
            header.encode(&mut buf);
            buf
        });
    });

    let mut encode_buf = BytesMut::with_capacity(24);
    header.encode(&mut encode_buf);
    let encoded_bytes = encode_buf.freeze();

    c.bench_function("frame_header_decode", |b| {
        b.iter(|| {
            let mut src = encoded_bytes.clone();
            FrameHeader::decode(&mut src).unwrap()
        });
    });
}

criterion_group!(benches, bench_frame_header_roundtrip);
criterion_main!(benches);
