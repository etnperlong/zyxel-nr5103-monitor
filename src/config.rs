use anyhow::{Context, Result};
use config::{Config as ConfigBuilder, File, FileFormat};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    pub router: RouterConfig,
    pub monitor: MonitorConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouterConfig {
    pub host: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MonitorConfig {
    #[serde(default = "default_interval_secs", with = "duration_secs")]
    pub interval: Duration,
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_timeout_secs", with = "duration_secs")]
    pub timeout: Duration,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_min_reboot_interval_secs", with = "duration_secs")]
    pub min_reboot_interval: Duration,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_interval_secs() -> Duration {
    Duration::from_secs(60)
}

fn default_url() -> String {
    "http://www.gstatic.com/generate_204".to_string()
}

fn default_timeout_secs() -> Duration {
    Duration::from_secs(5)
}

fn default_max_retries() -> u32 {
    1
}

fn default_min_reboot_interval_secs() -> Duration {
    Duration::from_secs(300)
}

mod duration_secs {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

pub fn load_config() -> Result<AppConfig> {
    let cfg = ConfigBuilder::builder()
        .add_source(File::with_name("config").format(FileFormat::Toml).required(false))
        .add_source(
            File::with_name(&format!(
                "{}/.config/nr5103/config",
                std::env::var("HOME").unwrap_or_default()
            ))
            .format(FileFormat::Toml)
            .required(false),
        )
        .add_source(
            File::with_name("/etc/nr5103/config")
                .format(FileFormat::Toml)
                .required(false),
        )
        .build()
        .context("Failed to build config")?;

    cfg.try_deserialize()
        .context("Failed to deserialize config")
}
