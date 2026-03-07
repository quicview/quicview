use thiserror::Error;

/// Errors from the QUIC transport layer.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[error("stream closed")]
    StreamClosed,

    #[error("stream open failed: {0}")]
    StreamOpenFailed(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("bind failed: {0}")]
    BindFailed(String),

    #[error("accept failed: {0}")]
    AcceptFailed(String),

    #[error("send failed: {0}")]
    SendFailed(String),

    #[error("receive failed: {0}")]
    RecvFailed(String),

    #[error("timeout")]
    Timeout,

    #[error("protocol error: {0}")]
    Protocol(#[from] quicview_protocol::ProtocolError),

    #[error("quinn connection error: {0}")]
    Quinn(#[from] quinn::ConnectionError),

    #[error("quinn write error: {0}")]
    WriteError(#[from] quinn::WriteError),
}
