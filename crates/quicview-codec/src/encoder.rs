use bytes::Bytes;

use crate::error::CodecError;

/// A frame encoder — compresses raw pixel data for transmission.
pub trait Encoder: Send {
    /// Encode raw pixels into a compressed payload.
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError>;
}

/// A frame decoder — decompresses received payloads back to pixels.
pub trait Decoder: Send {
    /// Decode a compressed payload back to raw pixels.
    fn decode(&mut self, compressed: &Bytes) -> Result<Vec<u8>, CodecError>;
}

/// Pass-through codec that ships raw uncompressed pixels.
///
/// Useful for local/LAN scenarios where bandwidth is plentiful and
/// latency from compression would be wasted.
pub struct RawCodec;

impl Encoder for RawCodec {
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError> {
        Ok(Bytes::copy_from_slice(raw))
    }
}

impl Decoder for RawCodec {
    fn decode(&mut self, compressed: &Bytes) -> Result<Vec<u8>, CodecError> {
        Ok(compressed.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_codec_roundtrip() {
        let data: Vec<u8> = (0..=255).collect();
        let mut enc = RawCodec;
        let compressed = enc.encode(&data).unwrap();

        let mut dec = RawCodec;
        let decoded = dec.decode(&compressed).unwrap();
        assert_eq!(decoded, data);
    }
}
