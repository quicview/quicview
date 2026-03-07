use quicview_protocol::InputEvent;

use crate::error::InputError;
use crate::injector::InputInjector;

/// Linux input injector stub (evdev / uinput).
///
/// Not yet implemented. Returns [`InputError::PlatformNotSupported`].
pub struct EvdevInputInjector;

impl EvdevInputInjector {
    pub fn new() -> Result<Self, InputError> {
        Err(InputError::PlatformNotSupported)
    }
}

impl InputInjector for EvdevInputInjector {
    fn inject(&mut self, _event: &InputEvent) -> Result<(), InputError> {
        Err(InputError::PlatformNotSupported)
    }
}
