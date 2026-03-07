use quicview_protocol::InputEvent;

use crate::error::InputError;
use crate::injector::InputInjector;

/// macOS input injector stub (CGEvent).
///
/// Not yet implemented. Returns [`InputError::PlatformNotSupported`].
pub struct CgEventInputInjector;

impl CgEventInputInjector {
    pub fn new() -> Result<Self, InputError> {
        Err(InputError::PlatformNotSupported)
    }
}

impl InputInjector for CgEventInputInjector {
    fn inject(&mut self, _event: &InputEvent) -> Result<(), InputError> {
        Err(InputError::PlatformNotSupported)
    }
}
