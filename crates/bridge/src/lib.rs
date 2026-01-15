use config::QuicViewConfig;

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("not implemented")] 
    NotImplemented,
    #[error("command not found: {0}")]
    CommandNotFound(String),
    #[error("launch failed: {0}")]
    LaunchFailed(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub trait ClientLauncher {
    fn launch_client(&self, cfg: &QuicViewConfig) -> Result<(), BridgeError>;
}

pub trait ServerLauncher {
    fn launch_servers(&self, cfg: &QuicViewConfig) -> Result<(), BridgeError>;
}

pub struct NoopLauncher;

impl ClientLauncher for NoopLauncher {
    fn launch_client(&self, _cfg: &QuicViewConfig) -> Result<(), BridgeError> {
        Err(BridgeError::NotImplemented)
    }
}

impl ServerLauncher for NoopLauncher {
    fn launch_servers(&self, _cfg: &QuicViewConfig) -> Result<(), BridgeError> {
        Err(BridgeError::NotImplemented)
    }
}
