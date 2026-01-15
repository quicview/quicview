//! Display Detection Module for QuicView Server
//!
//! Detects whether a display (X11, Wayland, or native) is available
//! for screen capture. Used to determine server capabilities at startup.

#[cfg(target_os = "linux")]
use std::env;

/// Result of display detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayInfo {
    /// Whether a display is available for capture
    pub available: bool,
    /// Type of display backend detected
    pub backend: DisplayBackend,
    /// Human-readable description
    pub description: String,
}

/// Display backend type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayBackend {
    /// No display available
    None,
    /// X11 display (Linux)
    X11,
    /// Wayland display (Linux)
    Wayland,
    /// Windows desktop
    Windows,
    /// macOS Quartz
    Quartz,
    /// Android (not typically used for server)
    Android,
}

impl std::fmt::Display for DisplayBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::X11 => write!(f, "x11"),
            Self::Wayland => write!(f, "wayland"),
            Self::Windows => write!(f, "windows"),
            Self::Quartz => write!(f, "quartz"),
            Self::Android => write!(f, "android"),
        }
    }
}

impl DisplayInfo {
    /// Create a new DisplayInfo indicating no display
    #[must_use]
    pub fn none(reason: &str) -> Self {
        Self {
            available: false,
            backend: DisplayBackend::None,
            description: reason.to_string(),
        }
    }

    /// Create a new DisplayInfo for an available display
    #[must_use]
    pub fn available(backend: DisplayBackend, desc: &str) -> Self {
        Self {
            available: true,
            backend,
            description: desc.to_string(),
        }
    }
}

/// Detect display availability on the current system.
///
/// This function checks for display availability based on the platform:
/// - **Linux**: Checks for X11 ($DISPLAY) or Wayland ($WAYLAND_DISPLAY)
/// - **Windows**: Always available (desktop is always present)
/// - **macOS**: Always available (Quartz is always present)
///
/// # Returns
/// A `DisplayInfo` struct with detection results.
#[must_use]
pub fn detect_display() -> DisplayInfo {
    #[cfg(target_os = "linux")]
    {
        detect_linux_display()
    }

    #[cfg(target_os = "windows")]
    {
        detect_windows_display()
    }

    #[cfg(target_os = "macos")]
    {
        detect_macos_display()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        DisplayInfo::none("Unsupported platform")
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_display() -> DisplayInfo {
    // Check for Wayland first (newer, preferred on modern systems)
    if let Ok(wayland_display) = env::var("WAYLAND_DISPLAY") {
        if !wayland_display.is_empty() {
            // Verify the socket exists
            let xdg_runtime = env::var("XDG_RUNTIME_DIR").unwrap_or_default();
            let socket_path = format!("{}/{}", xdg_runtime, wayland_display);
            if std::path::Path::new(&socket_path).exists() {
                return DisplayInfo::available(
                    DisplayBackend::Wayland,
                    &format!("Wayland display: {}", wayland_display),
                );
            }
        }
    }

    // Check for X11
    if let Ok(display) = env::var("DISPLAY") {
        if !display.is_empty() {
            // Try to verify X11 is actually accessible
            if verify_x11_connection(&display) {
                return DisplayInfo::available(
                    DisplayBackend::X11,
                    &format!("X11 display: {}", display),
                );
            } else {
                return DisplayInfo::none(&format!(
                    "DISPLAY={} set but X11 connection failed",
                    display
                ));
            }
        }
    }

    DisplayInfo::none("No DISPLAY or WAYLAND_DISPLAY environment variable set")
}

#[cfg(target_os = "linux")]
fn verify_x11_connection(display: &str) -> bool {
    // Quick check: try to parse the display string
    // Format is typically :N or host:N or host:N.S
    if display.is_empty() {
        return false;
    }

    // For now, just check if DISPLAY is set to something reasonable
    // A more thorough check would attempt xcb_connect, but that requires
    // linking against libxcb which we want to avoid at detection time
    
    // Check for common patterns
    if display.starts_with(':') {
        // Local display like :0 or :99
        let num_part = &display[1..];
        // Should be a number, optionally followed by .screen
        let display_num = num_part.split('.').next().unwrap_or("");
        return display_num.parse::<u32>().is_ok();
    }

    // Remote display like hostname:0
    if display.contains(':') {
        return true;
    }

    false
}

#[cfg(target_os = "windows")]
fn detect_windows_display() -> DisplayInfo {
    // On Windows, we can check if we're running in a desktop session
    // Windows Server Core without Desktop Experience won't have explorer.exe
    
    // Check for session type
    if is_windows_desktop_session() {
        DisplayInfo::available(DisplayBackend::Windows, "Windows desktop session")
    } else {
        DisplayInfo::none("Windows session without desktop (Server Core?)")
    }
}

#[cfg(target_os = "windows")]
fn is_windows_desktop_session() -> bool {
    use std::process::Command;
    
    // Quick heuristic: check if explorer.exe is running
    // This indicates a desktop session
    match Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq explorer.exe", "/NH"])
        .output()
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains("explorer.exe")
        }
        Err(_) => {
            // If we can't run tasklist, assume desktop is available
            // (better to try and fail than to refuse)
            true
        }
    }
}

#[cfg(target_os = "macos")]
fn detect_macos_display() -> DisplayInfo {
    // macOS always has Quartz available when running as a GUI app
    // Check if we're running in a GUI session
    
    if env::var("__CFBundleIdentifier").is_ok() {
        // Running as an app bundle
        return DisplayInfo::available(DisplayBackend::Quartz, "macOS app bundle");
    }
    
    // Check for window server
    if is_macos_gui_session() {
        DisplayInfo::available(DisplayBackend::Quartz, "macOS GUI session")
    } else {
        DisplayInfo::none("macOS without GUI session (SSH?)")
    }
}

#[cfg(target_os = "macos")]
fn is_macos_gui_session() -> bool {
    use std::process::Command;
    
    // Check if WindowServer is running
    match Command::new("pgrep")
        .args(["-x", "WindowServer"])
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => {
            // Fallback: check for DISPLAY or assume available
            true
        }
    }
}

/// Server capabilities based on display detection and configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCapabilities {
    /// Screen capture is available
    pub screen_capture: bool,
    /// Input injection is available
    pub input_control: bool,
    /// Terminal/shell access is available
    pub terminal: bool,
    /// Clipboard sync is available
    pub clipboard: bool,
    /// File transfer is available
    pub file_transfer: bool,
    /// Display info
    pub display_info: DisplayInfo,
    /// Effective mode the server is running in
    pub effective_mode: EffectiveMode,
}

/// The effective mode the server is running in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveMode {
    /// Full desktop mode with screen capture
    Desktop,
    /// Terminal-only mode (no screen capture)
    Terminal,
}

impl std::fmt::Display for EffectiveMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Desktop => write!(f, "desktop"),
            Self::Terminal => write!(f, "terminal"),
        }
    }
}

impl ServerCapabilities {
    /// Determine server capabilities based on display mode and detection
    #[must_use]
    pub fn detect(display_mode: config::DisplayMode) -> Self {
        let display_info = detect_display();
        
        let (effective_mode, screen_capture, input_control) = match display_mode {
            config::DisplayMode::Auto => {
                if display_info.available {
                    (EffectiveMode::Desktop, true, true)
                } else {
                    (EffectiveMode::Terminal, false, false)
                }
            }
            config::DisplayMode::Desktop => {
                // Desktop mode requires display
                (EffectiveMode::Desktop, display_info.available, display_info.available)
            }
            config::DisplayMode::Terminal => {
                // Terminal mode doesn't use display even if available
                (EffectiveMode::Terminal, false, false)
            }
        };

        Self {
            screen_capture,
            input_control,
            terminal: true, // Always available
            clipboard: screen_capture, // Clipboard typically requires display on Linux
            file_transfer: true, // Always available
            display_info,
            effective_mode,
        }
    }

    /// Check if the server should start based on capabilities and requirements
    #[must_use]
    pub fn can_start(&self, display_mode: config::DisplayMode) -> Result<(), String> {
        match display_mode {
            config::DisplayMode::Desktop => {
                if !self.display_info.available {
                    Err(format!(
                        "Display mode 'desktop' requires a display, but none found: {}",
                        self.display_info.description
                    ))
                } else {
                    Ok(())
                }
            }
            config::DisplayMode::Auto | config::DisplayMode::Terminal => {
                // These modes always allow starting
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_info_none() {
        let info = DisplayInfo::none("test reason");
        assert!(!info.available);
        assert_eq!(info.backend, DisplayBackend::None);
        assert_eq!(info.description, "test reason");
    }

    #[test]
    fn test_display_info_available() {
        let info = DisplayInfo::available(DisplayBackend::X11, "test display");
        assert!(info.available);
        assert_eq!(info.backend, DisplayBackend::X11);
    }

    #[test]
    fn test_capabilities_terminal_mode() {
        let caps = ServerCapabilities::detect(config::DisplayMode::Terminal);
        assert!(!caps.screen_capture);
        assert!(!caps.input_control);
        assert!(caps.terminal);
        assert_eq!(caps.effective_mode, EffectiveMode::Terminal);
    }

    #[test]
    fn test_can_start_terminal_mode() {
        let caps = ServerCapabilities {
            screen_capture: false,
            input_control: false,
            terminal: true,
            clipboard: false,
            file_transfer: true,
            display_info: DisplayInfo::none("no display"),
            effective_mode: EffectiveMode::Terminal,
        };
        
        assert!(caps.can_start(config::DisplayMode::Terminal).is_ok());
        assert!(caps.can_start(config::DisplayMode::Auto).is_ok());
        assert!(caps.can_start(config::DisplayMode::Desktop).is_err());
    }

    #[test]
    fn test_backend_display() {
        assert_eq!(format!("{}", DisplayBackend::X11), "x11");
        assert_eq!(format!("{}", DisplayBackend::Wayland), "wayland");
        assert_eq!(format!("{}", DisplayBackend::None), "none");
    }
}
