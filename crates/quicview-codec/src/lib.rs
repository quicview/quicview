//! # quicview-codec
//!
//! Frame encoding, decoding, and pixel-format conversion for QuicView.
//!
//! Provides a trait-based codec abstraction so that concrete encoders
//! (raw, delta, hardware H.264, …) can be plugged in at runtime.

pub mod bitrate;
pub mod convert;
pub mod delta;
pub mod encoder;
pub mod error;

pub use bitrate::BitrateController;
pub use convert::{bgra_to_rgba, rgba_to_rgb};
pub use delta::DeltaCodec;
pub use encoder::{Decoder, Encoder, RawCodec};
pub use error::CodecError;
