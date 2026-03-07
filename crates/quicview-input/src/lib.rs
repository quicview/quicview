pub mod audio;
pub mod clipboard;
pub mod error;
pub mod forwarder;
pub mod injector;

// Platform-specific input injectors.
#[cfg(target_os = "windows")]
pub mod sendinput;
#[cfg(target_os = "linux")]
pub mod evdev;
#[cfg(target_os = "macos")]
pub mod cgevent;

pub use audio::{AudioCapture, SilentAudioCapture};
pub use clipboard::{ClipboardProvider, MemoryClipboard};
pub use error::InputError;
pub use forwarder::InputForwarder;
pub use injector::{InputInjector, LogInjector};

// Platform injector re-exports.
#[cfg(target_os = "windows")]
pub use sendinput::WindowsInputInjector;
#[cfg(target_os = "linux")]
pub use evdev::EvdevInputInjector;
#[cfg(target_os = "macos")]
pub use cgevent::CgEventInputInjector;
