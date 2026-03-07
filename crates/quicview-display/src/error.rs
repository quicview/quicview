use thiserror::Error;

/// Errors produced by the display / rendering layer.
#[derive(Debug, Error)]
pub enum DisplayError {
    #[error("display not found: {0}")]
    DisplayNotFound(u32),

    #[error("surface creation failed: {0}")]
    SurfaceCreation(String),

    #[error("render failed: {0}")]
    RenderFailed(String),

    #[error("unsupported pixel format")]
    UnsupportedFormat,

    #[error("platform not supported")]
    PlatformNotSupported,
}
