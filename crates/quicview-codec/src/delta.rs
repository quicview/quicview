use bytes::Bytes;

use crate::error::CodecError;
use crate::encoder::{Decoder, Encoder};

/// XOR-based frame-differencing codec.
///
/// First frame is sent as a keyframe (raw pixels). Subsequent frames are
/// XOR'd against the previous frame so only changed bytes are non-zero.
/// The decoder reconstructs frames by applying the XOR delta.
///
/// This is a simple, zero-dependency approach to delta compression that
/// pairs well with a downstream entropy coder.
pub struct DeltaCodec {
    prev_frame: Option<Vec<u8>>,
}

impl DeltaCodec {
    pub fn new() -> Self {
        Self { prev_frame: None }
    }

    /// Reset the reference frame, forcing the next encode to produce a keyframe.
    pub fn reset(&mut self) {
        self.prev_frame = None;
    }
}

impl Default for DeltaCodec {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder for DeltaCodec {
    fn encode(&mut self, raw: &[u8]) -> Result<Bytes, CodecError> {
        match &self.prev_frame {
            None => {
                self.prev_frame = Some(raw.to_vec());
                Ok(Bytes::copy_from_slice(raw))
            }
            Some(prev) => {
                if prev.len() != raw.len() {
                    // Resolution or format changed — send keyframe.
                    self.prev_frame = Some(raw.to_vec());
                    return Ok(Bytes::copy_from_slice(raw));
                }
                let delta: Vec<u8> = raw
                    .iter()
                    .zip(prev.iter())
                    .map(|(a, b)| a ^ b)
                    .collect();
                self.prev_frame = Some(raw.to_vec());
                Ok(Bytes::from(delta))
            }
        }
    }
}

impl Decoder for DeltaCodec {
    fn decode(&mut self, compressed: &Bytes) -> Result<Vec<u8>, CodecError> {
        match &self.prev_frame {
            None => {
                let data = compressed.to_vec();
                self.prev_frame = Some(data.clone());
                Ok(data)
            }
            Some(prev) => {
                if prev.len() != compressed.len() {
                    let data = compressed.to_vec();
                    self.prev_frame = Some(data.clone());
                    return Ok(data);
                }
                let frame: Vec<u8> = compressed
                    .iter()
                    .zip(prev.iter())
                    .map(|(d, p)| d ^ p)
                    .collect();
                self.prev_frame = Some(frame.clone());
                Ok(frame)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_keyframe_roundtrip() {
        let data = vec![10, 20, 30, 40];
        let mut enc = DeltaCodec::new();
        let encoded = enc.encode(&data).unwrap();
        // First frame should be sent as-is.
        assert_eq!(encoded.as_ref(), &data);

        let mut dec = DeltaCodec::new();
        let decoded = dec.decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn delta_second_frame_only_changes() {
        let frame1 = vec![10, 20, 30, 40];
        let frame2 = vec![10, 25, 30, 40]; // byte 1 changed

        let mut enc = DeltaCodec::new();
        enc.encode(&frame1).unwrap();
        let delta = enc.encode(&frame2).unwrap();
        // XOR delta: only position 1 differs.
        assert_eq!(delta.as_ref(), &[0, 20 ^ 25, 0, 0]);

        let mut dec = DeltaCodec::new();
        dec.decode(&Bytes::from(frame1.clone())).unwrap();
        let reconstructed = dec.decode(&delta).unwrap();
        assert_eq!(reconstructed, frame2);
    }

    #[test]
    fn delta_resolution_change_resets() {
        let mut enc = DeltaCodec::new();
        enc.encode(&[1, 2, 3, 4]).unwrap();
        // Different length → keyframe.
        let encoded = enc.encode(&[5, 6]).unwrap();
        assert_eq!(encoded.as_ref(), &[5, 6]);
    }

    #[test]
    fn delta_reset_forces_keyframe() {
        let mut enc = DeltaCodec::new();
        enc.encode(&[1, 2, 3, 4]).unwrap();
        enc.reset();
        let encoded = enc.encode(&[5, 6, 7, 8]).unwrap();
        // After reset, should be a full keyframe.
        assert_eq!(encoded.as_ref(), &[5, 6, 7, 8]);
    }
}
