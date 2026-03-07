/// Errors from screen capture operations.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no display found with id {0}")]
    DisplayNotFound(u32),

    #[error("capture failed: {0}")]
    CaptureFailed(String),

    #[error("virtual display creation failed: {0}")]
    VirtualDisplayFailed(String),

    #[error("permission denied: {0}")]
    PermissionDenied(String),

    #[error("platform not supported: {0}")]
    PlatformNotSupported(String),
}
