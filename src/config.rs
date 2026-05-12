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
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RouterConfig {
    pub host: String,
    #[serde(default = "default_router_protocol")]
    pub protocol: String,
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
    #[serde(default = "default_recovery_method")]
    pub recovery_method: RecoveryMethod,
    #[serde(
        default = "default_access_technology_switch_wait_secs",
        with = "duration_secs"
    )]
    pub access_technology_switch_wait: Duration,
    #[serde(
        default = "default_access_technology_restore_wait_secs",
        with = "duration_secs"
    )]
    pub access_technology_restore_wait: Duration,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMethod {
    AccessTechnologySwitchThenReboot,
    Reboot,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelemetryConfig {
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default = "default_export_interval_secs", with = "duration_secs")]
    pub export_interval: Duration,
    #[serde(default)]
    pub metrics: TelemetrySignalConfig,
    #[serde(default)]
    pub traces: TelemetrySignalConfig,
    #[serde(default)]
    pub logs: TelemetrySignalConfig,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: default_service_name(),
            endpoint: None,
            export_interval: default_export_interval_secs(),
            metrics: TelemetrySignalConfig::default(),
            traces: TelemetrySignalConfig::default(),
            logs: TelemetrySignalConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct TelemetrySignalConfig {
    #[serde(default)]
    pub enabled: bool,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_router_protocol() -> String {
    "http".to_string()
}

fn default_service_name() -> String {
    "zyxel-nr5103-monitor".to_string()
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

fn default_export_interval_secs() -> Duration {
    Duration::from_secs(60)
}

fn default_max_retries() -> u32 {
    1
}

fn default_min_reboot_interval_secs() -> Duration {
    Duration::from_secs(300)
}

fn default_recovery_method() -> RecoveryMethod {
    RecoveryMethod::AccessTechnologySwitchThenReboot
}

fn default_access_technology_switch_wait_secs() -> Duration {
    Duration::from_secs(15)
}

fn default_access_technology_restore_wait_secs() -> Duration {
    Duration::from_secs(15)
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
        .add_source(
            File::with_name("config")
                .format(FileFormat::Toml)
                .required(false),
        )
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
