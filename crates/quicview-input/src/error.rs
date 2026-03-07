use thiserror::Error;

/// Errors from the input injection / forwarding layer.
#[derive(Debug, Error)]
pub enum InputError {
    #[error("injection failed: {0}")]
    InjectionFailed(String),

    #[error("input channel closed")]
    ChannelClosed,

    #[error("platform not supported")]
    PlatformNotSupported,

    #[error("permission denied: {0}")]
    PermissionDenied(String),
}
