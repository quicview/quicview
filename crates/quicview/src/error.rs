use thiserror::Error;

/// Top-level error type that composes errors from all sub-crates.
#[derive(Debug, Error)]
pub enum QuicViewError {
    #[error("protocol error: {0}")]
    Protocol(#[from] quicview_protocol::ProtocolError),

    #[error("codec error: {0}")]
    Codec(#[from] quicview_codec::CodecError),

    #[error("capture error: {0}")]
    Capture(#[from] quicview_capture::CaptureError),

    #[error("display error: {0}")]
    Display(#[from] quicview_display::DisplayError),

    #[error("input error: {0}")]
    Input(#[from] quicview_input::InputError),

    #[error("session error: {0}")]
    Session(#[from] quicview_session::SessionError),
}
