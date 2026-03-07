use std::path::Path;

use serde::{Deserialize, Serialize};

/// Application configuration, loadable from a TOML file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Network settings.
    pub network: NetworkConfig,
    /// Display settings.
    pub display: DisplayConfig,
    /// Codec settings.
    pub codec: CodecConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// Bind address for the host.
    pub bind_address: String,
    /// Port for QUIC connections.
    pub port: u16,
    /// Maximum concurrent viewers.
    pub max_connections: u32,
    /// Keep-alive interval in milliseconds.
    pub keepalive_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Target frame rate.
    pub target_fps: u32,
    /// Default resolution width.
    pub width: u32,
    /// Default resolution height.
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CodecConfig {
    /// Target bitrate in bits per second.
    pub target_bitrate_bps: u64,
    /// Keyframe interval in frames (0 = auto).
    pub keyframe_interval: u32,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".into(),
            port: 4433,
            max_connections: 16,
            keepalive_ms: 5000,
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            target_fps: 60,
            width: 1920,
            height: 1080,
        }
    }
}

impl Default for CodecConfig {
    fn default() -> Self {
        Self {
            target_bitrate_bps: 50_000_000,
            keyframe_interval: 0,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file. Returns default config if the file
    /// does not exist.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::Io(path.to_path_buf(), e))?;
        toml::from_str(&content).map_err(ConfigError::Parse)
    }

    /// Serialize this config to a TOML string.
    pub fn to_toml(&self) -> Result<String, ConfigError> {
        toml::to_string_pretty(self).map_err(ConfigError::Serialize)
    }

    /// Socket address string for binding.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.network.bind_address, self.network.port)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config from {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),

    #[error("failed to parse TOML config: {0}")]
    Parse(#[from] toml::de::Error),

    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn default_config_round_trips() {
        let config = Config::default();
        let toml_str = config.to_toml().unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.network.port, 4433);
        assert_eq!(parsed.display.target_fps, 60);
    }

    #[test]
    fn load_missing_file_returns_default() {
        let config = Config::load(Path::new("nonexistent.toml")).unwrap();
        assert_eq!(config.network.port, 4433);
    }

    #[test]
    fn partial_toml_uses_defaults() {
        let toml_str = r#"
[network]
port = 5555
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.network.port, 5555);
        assert_eq!(config.network.max_connections, 16); // default
        assert_eq!(config.display.target_fps, 60); // default
    }

    #[test]
    fn load_from_file() {
        let dir = std::env::temp_dir().join("quicview_test_config");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "[codec]\ntarget_bitrate_bps = 10000000").unwrap();
        drop(f);

        let config = Config::load(&path).unwrap();
        assert_eq!(config.codec.target_bitrate_bps, 10_000_000);
        assert_eq!(config.network.port, 4433); // default

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bind_addr_formatting() {
        let config = Config::default();
        assert_eq!(config.bind_addr(), "0.0.0.0:4433");
    }
}
