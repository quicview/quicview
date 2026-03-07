//! # quicview-codec
//!
//! Frame encoding, decoding, and pixel-format conversion for QuicView.
//!
//! Provides a trait-based codec abstraction so that concrete encoders
//! (raw, zstd, hardware H.264, …) can be plugged in at runtime.

pub mod convert;
pub mod encoder;
pub mod error;

pub use convert::{bgra_to_rgba, rgba_to_rgb};
pub use encoder::{Decoder, Encoder, RawCodec};
pub use error::CodecError;
