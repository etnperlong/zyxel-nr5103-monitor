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
    pub action: ActionConfig,
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
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_recovery_method")]
    pub recovery_method: RecoveryMethod,
    #[serde(default)]
    pub internet: InternetConfig,
    #[serde(default)]
    pub signal: SignalConfig,
}

impl MonitorConfig {
    pub fn internet_interval(&self) -> Duration {
        self.internet.interval.unwrap_or(self.interval)
    }

    pub fn internet_max_retries(&self) -> u32 {
        self.internet.max_retries.unwrap_or(self.max_retries)
    }

    pub fn signal_interval(&self) -> Duration {
        self.signal.interval.unwrap_or(self.interval)
    }

    pub fn signal_max_retries(&self) -> u32 {
        self.signal.max_retries.unwrap_or(self.max_retries)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct InternetConfig {
    #[serde(default = "default_url")]
    pub url: String,
    #[serde(default = "default_timeout_secs", with = "duration_secs")]
    pub timeout: Duration,
    #[serde(default, with = "optional_duration_secs")]
    pub interval: Option<Duration>,
    #[serde(default)]
    pub max_retries: Option<u32>,
}

impl Default for InternetConfig {
    fn default() -> Self {
        Self {
            url: default_url(),
            timeout: default_timeout_secs(),
            interval: None,
            max_retries: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ActionConfig {
    #[serde(default)]
    pub reboot: RebootConfig,
    #[serde(default)]
    pub reload: ReloadConfig,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryMethod {
    #[serde(alias = "access_technology_switch_then_reboot")]
    Reload,
    Reboot,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RebootConfig {
    #[serde(default = "default_min_reboot_interval_secs", with = "duration_secs")]
    pub min_interval: Duration,
    #[serde(default = "default_reboot_wait_after_secs", with = "duration_secs")]
    pub wait_after: Duration,
}

impl Default for RebootConfig {
    fn default() -> Self {
        Self {
            min_interval: default_min_reboot_interval_secs(),
            wait_after: default_reboot_wait_after_secs(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReloadConfig {
    #[serde(default = "default_reload_switch_wait_secs", with = "duration_secs")]
    pub switch_wait: Duration,
    #[serde(default = "default_reload_restore_wait_secs", with = "duration_secs")]
    pub restore_wait: Duration,
}

impl Default for ReloadConfig {
    fn default() -> Self {
        Self {
            switch_wait: default_reload_switch_wait_secs(),
            restore_wait: default_reload_restore_wait_secs(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SignalConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, with = "optional_duration_secs")]
    pub interval: Option<Duration>,
    #[serde(default)]
    pub require_5g: bool,
    #[serde(default = "default_signal_min_5g_rsrp")]
    pub min_5g_rsrp: f64,
    #[serde(default)]
    pub max_retries: Option<u32>,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval: None,
            require_5g: false,
            min_5g_rsrp: default_signal_min_5g_rsrp(),
            max_retries: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelemetryConfig {
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub authorization: Option<String>,
    #[serde(default)]
    pub protocol: TelemetryProtocol,
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
            authorization: None,
            protocol: TelemetryProtocol::Grpc,
            export_interval: default_export_interval_secs(),
            metrics: TelemetrySignalConfig::default(),
            traces: TelemetrySignalConfig::default(),
            logs: TelemetrySignalConfig::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TelemetryProtocol {
    #[serde(alias = "grpc")]
    #[default]
    Grpc,
    #[serde(rename = "http/protobuf", alias = "http-protobuf")]
    HttpProtobuf,
}

impl TelemetryProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grpc => "grpc",
            Self::HttpProtobuf => "http/protobuf",
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

fn default_reboot_wait_after_secs() -> Duration {
    Duration::from_secs(60)
}

fn default_recovery_method() -> RecoveryMethod {
    RecoveryMethod::Reload
}

fn default_reload_switch_wait_secs() -> Duration {
    Duration::from_secs(15)
}

fn default_reload_restore_wait_secs() -> Duration {
    Duration::from_secs(15)
}

fn default_signal_min_5g_rsrp() -> f64 {
    -110.0
}

mod duration_secs {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(d)?;
        Ok(Duration::from_secs(secs))
    }
}

mod optional_duration_secs {
    use serde::{Deserialize, Deserializer};
    use std::time::Duration;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        let secs = Option::<u64>::deserialize(d)?;
        Ok(secs.map(Duration::from_secs))
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
