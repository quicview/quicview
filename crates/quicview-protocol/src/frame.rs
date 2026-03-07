use bytes::{Buf, BufMut, Bytes, BytesMut};

use crate::display::{DisplayId, PixelFormat, Resolution};
use crate::error::ProtocolError;

/// Kind of frame being transmitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    /// Full frame (key frame / IDR).
    Full = 0,
    /// Delta frame (only changed regions).
    Delta = 1,
    /// Cursor-only update (small, high frequency).
    Cursor = 2,
}

impl FrameKind {
    pub fn from_u8(v: u8) -> Result<Self, ProtocolError> {
        match v {
            0 => Ok(Self::Full),
            1 => Ok(Self::Delta),
            2 => Ok(Self::Cursor),
            other => Err(ProtocolError::UnknownFrameKind(other)),
        }
    }
}

/// Header prepended to every video frame on the wire.
///
/// Wire layout (24 bytes, big-endian):
/// ```text
/// [kind:1][display_id:4][seq:4][width:4][height:4][pixel_fmt:1][reserved:2][payload_len:4]
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameHeader {
    pub kind: FrameKind,
    pub display_id: DisplayId,
    pub sequence: u32,
    pub resolution: Resolution,
    pub pixel_format: PixelFormat,
    pub payload_len: u32,
}

/// Encoded size of a frame header.
pub const FRAME_HEADER_SIZE: usize = 24;

/// Maximum frame payload (64 MiB).
pub const MAX_PAYLOAD: usize = 64 * 1024 * 1024;

impl FrameHeader {
    /// Encode the header into bytes.
    pub fn encode(&self, dst: &mut BytesMut) {
        dst.put_u8(self.kind as u8);
        dst.put_u32(self.display_id.0);
        dst.put_u32(self.sequence);
        dst.put_u32(self.resolution.width);
        dst.put_u32(self.resolution.height);
        dst.put_u8(pixel_format_to_u8(self.pixel_format));
        dst.put_u16(0); // reserved
        dst.put_u32(self.payload_len);
    }

    /// Decode a header from bytes.
    pub fn decode(src: &mut Bytes) -> Result<Self, ProtocolError> {
        if src.remaining() < FRAME_HEADER_SIZE {
            return Err(ProtocolError::InvalidFrame(format!(
                "need {} bytes, got {}",
                FRAME_HEADER_SIZE,
                src.remaining()
            )));
        }

        let kind = FrameKind::from_u8(src.get_u8())?;
        let display_id = DisplayId(src.get_u32());
        let sequence = src.get_u32();
        let width = src.get_u32();
        let height = src.get_u32();
        let pixel_format = pixel_format_from_u8(src.get_u8())?;
        let _reserved = src.get_u16();
        let payload_len = src.get_u32();

        if payload_len as usize > MAX_PAYLOAD {
            return Err(ProtocolError::PayloadTooLarge {
                size: payload_len as usize,
                max: MAX_PAYLOAD,
            });
        }

        Ok(Self {
            kind,
            display_id,
            sequence,
            resolution: Resolution::new(width, height),
            pixel_format,
            payload_len,
        })
    }
}

fn pixel_format_to_u8(fmt: PixelFormat) -> u8 {
    match fmt {
        PixelFormat::Bgra8 => 0,
        PixelFormat::Rgba8 => 1,
        PixelFormat::Rgb8 => 2,
        PixelFormat::Nv12 => 3,
    }
}

fn pixel_format_from_u8(v: u8) -> Result<PixelFormat, ProtocolError> {
    match v {
        0 => Ok(PixelFormat::Bgra8),
        1 => Ok(PixelFormat::Rgba8),
        2 => Ok(PixelFormat::Rgb8),
        3 => Ok(PixelFormat::Nv12),
        _ => Err(ProtocolError::Decode(format!("unknown pixel format: {v}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_roundtrip() {
        let header = FrameHeader {
            kind: FrameKind::Full,
            display_id: DisplayId(1),
            sequence: 42,
            resolution: Resolution::new(1920, 1080),
            pixel_format: PixelFormat::Bgra8,
            payload_len: 8294400,
        };

        let mut buf = BytesMut::with_capacity(FRAME_HEADER_SIZE);
        header.encode(&mut buf);
        assert_eq!(buf.len(), FRAME_HEADER_SIZE);

        let mut bytes = buf.freeze();
        let decoded = FrameHeader::decode(&mut bytes).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn frame_header_rejects_huge_payload() {
        let header = FrameHeader {
            kind: FrameKind::Full,
            display_id: DisplayId::PRIMARY,
            sequence: 0,
            resolution: Resolution::new(1920, 1080),
            pixel_format: PixelFormat::Bgra8,
            payload_len: (MAX_PAYLOAD + 1) as u32,
        };

        let mut buf = BytesMut::with_capacity(FRAME_HEADER_SIZE);
        header.encode(&mut buf);
        let mut bytes = buf.freeze();
        let err = FrameHeader::decode(&mut bytes).unwrap_err();
        assert!(matches!(err, ProtocolError::PayloadTooLarge { .. }));
    }
}
