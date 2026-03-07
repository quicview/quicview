use thiserror::Error;

/// Errors from the session management layer.
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("negotiation failed: {0}")]
    NegotiationFailed(String),

    #[error("session already closed")]
    AlreadyClosed,

    #[error("invalid role transition: {from:?} -> {to:?}")]
    InvalidRoleTransition { from: String, to: String },

    #[error("timeout")]
    Timeout,
}
