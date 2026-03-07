use criterion::{Criterion, criterion_group, criterion_main};
use quicview_codec::{RawCodec, Encoder, Decoder};
use quicview_codec::convert::{bgra_to_rgba, rgba_to_rgb};

fn bench_raw_codec(c: &mut Criterion) {
    let mut codec = RawCodec;
    let frame = vec![0xABu8; 1920 * 1080 * 4];

    c.bench_function("raw_encode_1080p", |b| {
        b.iter(|| codec.encode(&frame).unwrap());
    });

    let encoded = codec.encode(&frame).unwrap();
    c.bench_function("raw_decode_1080p", |b| {
        b.iter(|| codec.decode(&encoded).unwrap());
    });
}

fn bench_pixel_convert(c: &mut Criterion) {
    let bgra = vec![0xAAu8; 1920 * 1080 * 4];

    c.bench_function("bgra_to_rgba_1080p", |b| {
        b.iter_batched(
            || bgra.clone(),
            |mut data| bgra_to_rgba(&mut data),
            criterion::BatchSize::LargeInput,
        );
    });

    let rgba = vec![0xBBu8; 1920 * 1080 * 4];
    c.bench_function("rgba_to_rgb_1080p", |b| {
        b.iter(|| rgba_to_rgb(&rgba));
    });
}

criterion_group!(benches, bench_raw_codec, bench_pixel_convert);
criterion_main!(benches);
