/// Errors from protocol encoding / decoding.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid frame: {0}")]
    InvalidFrame(String),

    #[error("unknown frame kind: {0}")]
    UnknownFrameKind(u8),

    #[error("unknown input event type: {0}")]
    UnknownInputEvent(u8),

    #[error("payload too large: {size} bytes (max {max})")]
    PayloadTooLarge { size: usize, max: usize },

    #[error("invalid display id: {0}")]
    InvalidDisplayId(u32),

    #[error("decode error: {0}")]
    Decode(String),
}
