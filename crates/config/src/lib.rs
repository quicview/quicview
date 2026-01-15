use serde::{Deserialize, Serialize};
use std::path::Path;

/// Display mode for the server.
/// 
/// Controls how the server handles screen capture and display availability.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DisplayMode {
    /// Automatically detect display availability.
    /// Falls back to terminal-only mode if no display is available.
    #[default]
    Auto,
    /// Require a display for screen capture.
    /// Server will fail to start if no display is available.
    Desktop,
    /// Terminal-only mode. No screen capture, only shell access.
    Terminal,
}

impl DisplayMode {
    /// Returns true if this mode allows terminal-only operation.
    #[must_use]
    pub fn allows_terminal_fallback(&self) -> bool {
        matches!(self, Self::Auto | Self::Terminal)
    }
    
    /// Returns true if this mode requires a display.
    #[must_use]
    pub fn requires_display(&self) -> bool {
        matches!(self, Self::Desktop)
    }
    
    /// Returns true if this is terminal-only mode.
    #[must_use]
    pub fn is_terminal_only(&self) -> bool {
        matches!(self, Self::Terminal)
    }
}

impl std::fmt::Display for DisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Desktop => write!(f, "desktop"),
            Self::Terminal => write!(f, "terminal"),
        }
    }
}

/// TLS configuration for the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TlsConfig {
    /// Enable TLS for incoming connections.
    #[serde(default)]
    pub enabled: bool,
    /// Path to PEM-encoded certificate chain.
    #[serde(default)]
    pub cert_path: Option<String>,
    /// Path to PEM-encoded private key.
    #[serde(default)]
    pub key_path: Option<String>,
    /// Optional CA bundle for client verification (mutual TLS).
    #[serde(default)]
    pub ca_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerConfig {
    // ===== NEW FIELDS (Client-Server Architecture) =====
    
    /// Bind address (IP or hostname). Defaults to "0.0.0.0".
    #[serde(default = "default_host")]
    pub host: String,
    
    /// Listen port. Defaults to 21116.
    #[serde(default = "default_port")]
    pub port: u16,
    
    /// Display mode: auto, desktop, or terminal.
    /// - auto: Detect display, fallback to terminal if unavailable
    /// - desktop: Require display, fail if unavailable  
    /// - terminal: Terminal-only, no screen capture
    #[serde(default)]
    pub display_mode: DisplayMode,
    
    /// Authentication token for client connections.
    /// Can also be set via QUICVIEW_TOKEN environment variable.
    #[serde(default)]
    pub auth_token: Option<String>,
    
    /// TLS configuration (nested for clarity).
    #[serde(default)]
    pub tls_config: Option<TlsConfig>,
    
    // ===== LEGACY FIELDS (P2P Architecture - DEPRECATED) =====
    // These fields are kept for backward compatibility during migration.
    // They will be removed in a future version.
    
    /// DEPRECATED: Use `host` and `port` instead.
    #[serde(default)]
    pub rendezvous_host: Option<String>,
    
    /// DEPRECATED: Relay is no longer used in client-server architecture.
    #[serde(default)]
    pub relay_host: Option<String>,
    
    /// DEPRECATED: Use `tls_config.enabled` instead.
    #[serde(default)]
    pub tls: Option<bool>,
    
    /// DEPRECATED: License key for upstream hbbs/hbbr compatibility.
    #[serde(default)]
    pub license_key: Option<String>,
    
    /// DEPRECATED: Relay servers list.
    #[serde(default)]
    pub relay_servers: Option<String>,
    
    /// DEPRECATED: Use `tls_config.cert_path` instead.
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    
    /// DEPRECATED: Use `tls_config.key_path` instead.
    #[serde(default)]
    pub tls_key_path: Option<String>,
    
    /// DEPRECATED: Use `host` instead.
    #[serde(default)]
    pub rendezvous_bind: Option<String>,
    
    /// DEPRECATED: Relay bind is no longer used.
    #[serde(default)]
    pub relay_bind: Option<String>,
    
    /// Health endpoint bind address (e.g., "127.0.0.1").
    #[serde(default)]
    pub health_bind: Option<String>,
    
    /// Health endpoint port (e.g., 21110).
    #[serde(default)]
    pub health_port: Option<u16>,
    
    /// QUIC server bind address (e.g., "0.0.0.0"). Defaults to "0.0.0.0".
    #[serde(default)]
    pub quic_bind: Option<String>,
    
    /// Client-side TLS CA bundle path.
    #[serde(default)]
    pub client_tls_ca_path: Option<String>,
    
    /// Client-side SNI override.
    #[serde(default)]
    pub client_tls_sni: Option<String>,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    21116
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            display_mode: DisplayMode::default(),
            auth_token: None,
            tls_config: None,
            rendezvous_host: None,
            relay_host: None,
            tls: None,
            license_key: None,
            relay_servers: None,
            tls_cert_path: None,
            tls_key_path: None,
            rendezvous_bind: None,
            relay_bind: None,
            health_bind: None,
            health_port: None,
            quic_bind: None,
            client_tls_ca_path: None,
            client_tls_sni: None,
        }
    }
}

impl ServerConfig {
    /// Get effective bind address, preferring new `host` field over legacy `rendezvous_bind`.
    #[must_use]
    pub fn effective_host(&self) -> &str {
        // If host is the default and rendezvous_bind is set, use legacy
        if self.host == "0.0.0.0" {
            if let Some(ref bind) = self.rendezvous_bind {
                return bind;
            }
        }
        &self.host
    }
    
    /// Get effective port, extracting from legacy `rendezvous_host` if needed.
    #[must_use]
    pub fn effective_port(&self) -> u16 {
        // If port is default and rendezvous_host contains a port, extract it
        if self.port == 21116 {
            if let Some(ref rh) = self.rendezvous_host {
                if let Some((_host, port_str)) = rh.rsplit_once(':') {
                    if let Ok(p) = port_str.parse::<u16>() {
                        return p;
                    }
                }
            }
        }
        self.port
    }
    
    /// Get effective TLS enabled state.
    #[must_use]
    pub fn effective_tls_enabled(&self) -> bool {
        if let Some(ref tc) = self.tls_config {
            return tc.enabled;
        }
        self.tls.unwrap_or(false)
    }
    
    /// Get effective TLS cert path.
    #[must_use]
    pub fn effective_tls_cert_path(&self) -> Option<&str> {
        if let Some(ref tc) = self.tls_config {
            return tc.cert_path.as_deref();
        }
        self.tls_cert_path.as_deref()
    }
    
    /// Get effective TLS key path.
    #[must_use]
    pub fn effective_tls_key_path(&self) -> Option<&str> {
        if let Some(ref tc) = self.tls_config {
            return tc.key_path.as_deref();
        }
        self.tls_key_path.as_deref()
    }
    
    /// Get effective client TLS SNI.
    #[must_use]
    pub fn effective_tls_sni(&self) -> Option<String> {
        self.client_tls_sni.clone()
    }
    
    /// Get effective client TLS CA file path.
    #[must_use]
    pub fn effective_tls_ca_file(&self) -> Option<String> {
        // Check tls_config.ca_path first, then legacy client_tls_ca_path
        if let Some(ref tc) = self.tls_config {
            if tc.ca_path.is_some() {
                return tc.ca_path.clone();
            }
        }
        self.client_tls_ca_path.clone()
    }
    
    /// Check if config uses deprecated P2P fields.
    #[must_use]
    pub fn has_deprecated_fields(&self) -> bool {
        self.rendezvous_host.is_some()
            || self.relay_host.is_some()
            || self.relay_servers.is_some()
            || self.rendezvous_bind.is_some()
            || self.relay_bind.is_some()
    }
    
    /// Return deprecation warnings for logging.
    #[must_use]
    pub fn deprecation_warnings(&self) -> Vec<&'static str> {
        let mut warnings = Vec::new();
        if self.rendezvous_host.is_some() {
            warnings.push("'rendezvous_host' is deprecated; use 'host' and 'port' instead");
        }
        if self.relay_host.is_some() {
            warnings.push("'relay_host' is deprecated; relay is no longer used");
        }
        if self.relay_servers.is_some() {
            warnings.push("'relay_servers' is deprecated; relay is no longer used");
        }
        if self.rendezvous_bind.is_some() {
            warnings.push("'rendezvous_bind' is deprecated; use 'host' instead");
        }
        if self.relay_bind.is_some() {
            warnings.push("'relay_bind' is deprecated; relay is no longer used");
        }
        if self.tls.is_some() {
            warnings.push("'tls' is deprecated; use 'tls_config.enabled' instead");
        }
        if self.tls_cert_path.is_some() {
            warnings.push("'tls_cert_path' is deprecated; use 'tls_config.cert_path' instead");
        }
        if self.tls_key_path.is_some() {
            warnings.push("'tls_key_path' is deprecated; use 'tls_config.key_path' instead");
        }
        warnings
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ClientPolicy {
    #[serde(default)]
    pub require_consent: bool,
    #[serde(default)]
    pub allow_input_control: bool,
    #[serde(default)]
    pub allow_clipboard: bool,
    #[serde(default)]
    pub allow_file_transfer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuicViewConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub client_policy: ClientPolicy,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

impl Default for ClientPolicy {
    fn default() -> Self {
        Self {
            require_consent: true,
            allow_input_control: false,
            allow_clipboard: false,
            allow_file_transfer: false,
        }
    }
}

impl QuicViewConfig {
    /// Load a `QuicViewConfig` from a YAML file at `path`.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or the YAML cannot be parsed.
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let cfg: Self = serde_yaml::from_str(&content)?;
        Ok(cfg)
    }
}
