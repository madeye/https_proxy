//! YAML configuration loading and validation.
//!
//! The proxy is configured via a `config.yaml` file. See [`Config`] for the
//! top-level structure and [`Config::load`] to read from disk.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level proxy configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// Address and port to bind (e.g. `"0.0.0.0:443"`).
    pub listen: String,
    /// Domain name for the TLS certificate.
    pub domain: String,
    /// ACME / Let's Encrypt settings.
    pub acme: AcmeConfig,
    /// Authorized proxy users.
    pub users: Vec<UserConfig>,
    /// Stealth-mode settings for fake responses.
    #[serde(default)]
    pub stealth: StealthConfig,
    /// Enable TCP Fast Open on listener and outgoing connections.
    #[serde(default)]
    pub fast_open: bool,
}

/// ACME certificate configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AcmeConfig {
    /// Contact email for Let's Encrypt.
    pub email: String,
    /// Use the Let's Encrypt staging environment.
    #[serde(default)]
    pub staging: bool,
    /// Directory to cache ACME account keys and certificates.
    pub cache_dir: PathBuf,
}

/// An authorized proxy user.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UserConfig {
    /// Username for Basic auth.
    pub username: String,
    /// Password for Basic auth.
    pub password: String,
}

/// Stealth-mode configuration.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct StealthConfig {
    /// `Server` header value in fake responses (e.g. `"nginx/1.24.0"`).
    #[serde(default = "default_server_name")]
    pub server_name: String,
}

impl Default for StealthConfig {
    fn default() -> Self {
        Self {
            server_name: default_server_name(),
        }
    }
}

fn default_server_name() -> String {
    "nginx/1.24.0".to_string()
}

impl Config {
    /// Load and parse configuration from a YAML file.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Serialize and write configuration to a YAML file.
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        let content = serde_yaml::to_string(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
