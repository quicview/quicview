use quicview_protocol::DisplayId;

use crate::error::SessionError;

/// Power state of a virtual display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// Display is active and streaming.
    Active,
    /// Display is sleeping (no frames sent, low power).
    Sleep,
    /// Display is waking up (transition state).
    Waking,
}

/// Manages power states for virtual displays.
///
/// Tracks idle time per display and transitions to sleep after an
/// inactivity timeout. Wakes displays on any input or connection event.
pub struct PowerManager {
    timeout_ms: u64,
    states: Vec<(DisplayId, PowerState, u64)>, // (id, state, last_active_ms)
}

impl PowerManager {
    /// Create a power manager with the given sleep timeout in milliseconds.
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            timeout_ms,
            states: Vec::new(),
        }
    }

    /// Register a display for power management.
    pub fn register(&mut self, display_id: DisplayId, now_ms: u64) {
        self.states.push((display_id, PowerState::Active, now_ms));
    }

    /// Mark a display as active (e.g. frame sent, input received).
    pub fn touch(&mut self, display_id: DisplayId, now_ms: u64) {
        for entry in &mut self.states {
            if entry.0 == display_id {
                entry.1 = PowerState::Active;
                entry.2 = now_ms;
                return;
            }
        }
    }

    /// Tick the power manager at the current timestamp.
    /// Returns display IDs that transitioned to sleep.
    pub fn tick(&mut self, now_ms: u64) -> Vec<DisplayId> {
        let mut newly_sleeping = Vec::new();
        for entry in &mut self.states {
            if entry.1 == PowerState::Active && now_ms.saturating_sub(entry.2) > self.timeout_ms {
                entry.1 = PowerState::Sleep;
                newly_sleeping.push(entry.0);
            }
        }
        newly_sleeping
    }

    /// Wake a sleeping display.
    pub fn wake(&mut self, display_id: DisplayId, now_ms: u64) -> Result<(), SessionError> {
        for entry in &mut self.states {
            if entry.0 == display_id {
                entry.1 = PowerState::Active;
                entry.2 = now_ms;
                return Ok(());
            }
        }
        Err(SessionError::NegotiationFailed(format!(
            "display {display_id} not registered for power management"
        )))
    }

    /// Get the power state of a display.
    pub fn state(&self, display_id: DisplayId) -> Option<PowerState> {
        self.states
            .iter()
            .find(|e| e.0 == display_id)
            .map(|e| e.1)
    }

    /// List all displays currently sleeping.
    pub fn sleeping_displays(&self) -> Vec<DisplayId> {
        self.states
            .iter()
            .filter(|e| e.1 == PowerState::Sleep)
            .map(|e| e.0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_sleeps_after_timeout() {
        let mut pm = PowerManager::new(5000);
        let id = DisplayId(1);
        pm.register(id, 0);

        assert_eq!(pm.state(id), Some(PowerState::Active));
        assert!(pm.tick(4000).is_empty());
        assert_eq!(pm.state(id), Some(PowerState::Active));

        let sleeping = pm.tick(6000);
        assert_eq!(sleeping, vec![id]);
        assert_eq!(pm.state(id), Some(PowerState::Sleep));
    }

    #[test]
    fn touch_resets_idle_timer() {
        let mut pm = PowerManager::new(5000);
        let id = DisplayId(1);
        pm.register(id, 0);

        pm.touch(id, 4000);
        assert!(pm.tick(8000).is_empty()); // 8000 - 4000 = 4000 < 5000

        let sleeping = pm.tick(10000);
        assert_eq!(sleeping, vec![id]);
    }

    #[test]
    fn wake_sleeping_display() {
        let mut pm = PowerManager::new(1000);
        let id = DisplayId(2);
        pm.register(id, 0);
        pm.tick(2000);
        assert_eq!(pm.state(id), Some(PowerState::Sleep));

        pm.wake(id, 3000).unwrap();
        assert_eq!(pm.state(id), Some(PowerState::Active));
    }
}
