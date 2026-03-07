/// Errors from frame encoding / decoding.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("encode error: {0}")]
    Encode(String),

    #[error("decode error: {0}")]
    Decode(String),

    #[error("unsupported pixel format conversion: {from} → {to}")]
    UnsupportedConversion { from: String, to: String },

    #[error("buffer size mismatch: expected {expected}, got {actual}")]
    BufferMismatch { expected: usize, actual: usize },
}
