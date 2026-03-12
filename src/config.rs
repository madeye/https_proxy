use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub listen: String,
    pub domain: String,
    pub acme: AcmeConfig,
    pub users: Vec<UserConfig>,
    #[serde(default)]
    pub stealth: StealthConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AcmeConfig {
    pub email: String,
    #[serde(default)]
    pub staging: bool,
    pub cache_dir: PathBuf,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UserConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StealthConfig {
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
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
