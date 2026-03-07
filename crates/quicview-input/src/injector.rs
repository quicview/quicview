use quicview_protocol::InputEvent;

use crate::error::InputError;

/// Trait for injecting input events into the host operating system.
///
/// Implementations translate [`InputEvent`] values into platform-native
/// input (e.g. `SendInput` on Windows, `evdev` on Linux, CGEvent on macOS).
pub trait InputInjector: Send {
    /// Inject a single input event.
    fn inject(&mut self, event: &InputEvent) -> Result<(), InputError>;

    /// Inject a batch of events atomically (best-effort).
    fn inject_batch(&mut self, events: &[InputEvent]) -> Result<(), InputError> {
        for event in events {
            self.inject(event)?;
        }
        Ok(())
    }
}

/// An injector that logs events via [`tracing`] without touching the OS.
/// Useful for tests and headless hosts.
pub struct LogInjector {
    events_injected: u64,
}

impl LogInjector {
    pub fn new() -> Self {
        Self { events_injected: 0 }
    }

    pub fn events_injected(&self) -> u64 {
        self.events_injected
    }
}

impl Default for LogInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl InputInjector for LogInjector {
    fn inject(&mut self, event: &InputEvent) -> Result<(), InputError> {
        tracing::debug!(?event, "injected input event");
        self.events_injected += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quicview_protocol::{KeyAction, KeyEvent, MouseButton, MouseEvent};

    #[test]
    fn log_injector_counts() {
        let mut inj = LogInjector::new();
        let events = vec![
            InputEvent::Mouse(MouseEvent {
                x: 100,
                y: 200,
                button: Some((MouseButton::Left, KeyAction::Press)),
            }),
            InputEvent::Key(KeyEvent {
                keycode: 0x04, // HID 'A'
                action: KeyAction::Press,
                modifiers: 0,
            }),
        ];
        inj.inject_batch(&events).unwrap();
        assert_eq!(inj.events_injected(), 2);
    }
}
