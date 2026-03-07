use std::sync::Arc;

use tokio::sync::watch;

/// A cooperative shutdown signal.
///
/// Call [`ShutdownController::trigger`] to initiate shutdown.
/// Distribute [`ShutdownSignal`] handles to tasks that should
/// react to the shutdown event.
#[derive(Debug, Clone)]
pub struct ShutdownController {
    tx: Arc<watch::Sender<bool>>,
}

/// A handle that tasks hold to check whether shutdown has been requested.
#[derive(Debug, Clone)]
pub struct ShutdownSignal {
    rx: watch::Receiver<bool>,
}

impl ShutdownController {
    /// Create a new shutdown controller and its signal.
    pub fn new() -> (Self, ShutdownSignal) {
        let (tx, rx) = watch::channel(false);
        (Self { tx: Arc::new(tx) }, ShutdownSignal { rx })
    }

    /// Request shutdown. All associated signals will be notified.
    pub fn trigger(&self) {
        let _ = self.tx.send(true);
    }

    /// Create an additional signal handle.
    pub fn signal(&self) -> ShutdownSignal {
        ShutdownSignal {
            rx: self.tx.subscribe(),
        }
    }
}

impl Default for ShutdownController {
    fn default() -> Self {
        Self::new().0
    }
}

impl ShutdownSignal {
    /// Wait until shutdown is requested.
    pub async fn wait(&mut self) {
        while !*self.rx.borrow() {
            if self.rx.changed().await.is_err() {
                // Sender dropped — treat as shutdown.
                return;
            }
        }
    }

    /// Check if shutdown has been requested (non-blocking).
    pub fn is_shutdown(&self) -> bool {
        *self.rx.borrow()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shutdown_signal_triggers() {
        let (ctrl, mut sig) = ShutdownController::new();
        assert!(!sig.is_shutdown());

        ctrl.trigger();
        sig.wait().await;
        assert!(sig.is_shutdown());
    }

    #[tokio::test]
    async fn multiple_signals() {
        let (ctrl, _sig1) = ShutdownController::new();
        let mut sig2 = ctrl.signal();
        let mut sig3 = ctrl.signal();

        ctrl.trigger();
        sig2.wait().await;
        sig3.wait().await;
        assert!(sig2.is_shutdown());
        assert!(sig3.is_shutdown());
    }

    #[tokio::test]
    async fn dropped_controller_unblocks_signal() {
        let (ctrl, mut sig) = ShutdownController::new();
        drop(ctrl);
        sig.wait().await; // should return immediately
    }
}
