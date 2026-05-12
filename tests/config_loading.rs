use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use zyxel_nr5103_monitor::config::{self, RecoveryMethod};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env() -> MutexGuard<'static, ()> {
    env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct CurrentDirGuard {
    original: PathBuf,
}

impl CurrentDirGuard {
    fn change_to(path: &Path) -> Self {
        let original = std::env::current_dir().expect("current dir should be available");
        std::env::set_current_dir(path).expect("current dir should be changed for test");
        Self { original }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.original).expect("current dir should be restored");
    }
}

fn unique_temp_dir() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("nr5103-config-test-{timestamp}"));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

#[test]
fn load_config_uses_default_monitor_values_when_omitted() {
    let _env_guard = lock_env();
    let temp_dir = unique_temp_dir();
    let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

    fs::write(
        temp_dir.join("config.toml"),
        r#"
log_level = "debug"

[router]
host = "172.16.0.1"
username = "monitor"
password = "secret"

[monitor]
"#,
    )
    .expect("config file should be written");

    let loaded = config::load_config().expect("config should load from local file");

    assert_eq!(loaded.log_level, "debug");
    assert_eq!(loaded.router.protocol, "http");
    assert_eq!(loaded.monitor.interval, Duration::from_secs(60));
    assert_eq!(loaded.monitor.url, "http://www.gstatic.com/generate_204");
    assert_eq!(loaded.monitor.timeout, Duration::from_secs(5));
    assert_eq!(loaded.monitor.max_retries, 1);
    assert_eq!(loaded.monitor.reboot.min_interval, Duration::from_secs(300));
    assert_eq!(loaded.monitor.reboot.wait_after, Duration::from_secs(60));
    assert_eq!(loaded.monitor.recovery_method, RecoveryMethod::Reload);
    assert_eq!(loaded.monitor.reload.switch_wait, Duration::from_secs(15));
    assert_eq!(loaded.monitor.reload.restore_wait, Duration::from_secs(15));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_config_allows_explicit_router_protocol() {
    let _env_guard = lock_env();
    let temp_dir = unique_temp_dir();
    let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

    fs::write(
        temp_dir.join("config.toml"),
        r#"
[router]
host = "172.16.0.1"
protocol = "https"
username = "monitor"
password = "secret"

[monitor]
"#,
    )
    .expect("config file should be written");

    let loaded = config::load_config().expect("config should load from local file");

    assert_eq!(loaded.router.protocol, "https");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_config_allows_explicit_recovery_method() {
    let _env_guard = lock_env();
    let temp_dir = unique_temp_dir();
    let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

    fs::write(
        temp_dir.join("config.toml"),
        r#"
[router]
host = "172.16.0.1"
username = "monitor"
password = "secret"

[monitor]
recovery_method = "reload"

[monitor.reboot]
min_interval = 123
wait_after = 45

[monitor.reload]
switch_wait = 9
restore_wait = 11
"#,
    )
    .expect("config file should be written");

    let loaded = config::load_config().expect("config should load with explicit recovery settings");

    assert_eq!(loaded.monitor.recovery_method, RecoveryMethod::Reload);
    assert_eq!(loaded.monitor.reboot.min_interval, Duration::from_secs(123));
    assert_eq!(loaded.monitor.reboot.wait_after, Duration::from_secs(45));
    assert_eq!(loaded.monitor.reload.switch_wait, Duration::from_secs(9));
    assert_eq!(loaded.monitor.reload.restore_wait, Duration::from_secs(11));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_config_accepts_legacy_reload_recovery_method_name() {
    let _env_guard = lock_env();
    let temp_dir = unique_temp_dir();
    let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

    fs::write(
        temp_dir.join("config.toml"),
        r#"
[router]
host = "172.16.0.1"
username = "monitor"
password = "secret"

[monitor]
recovery_method = "access_technology_switch_then_reboot"
"#,
    )
    .expect("config file should be written");

    let loaded =
        config::load_config().expect("config should load with the legacy reload recovery name");

    assert_eq!(loaded.monitor.recovery_method, RecoveryMethod::Reload);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn config_telemetry_defaults_disabled_when_omitted() {
    let _env_guard = lock_env();
    let temp_dir = unique_temp_dir();
    let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

    fs::write(
        temp_dir.join("config.toml"),
        r#"
log_level = "debug"

[router]
host = "172.16.0.1"
username = "monitor"
password = "secret"

[monitor]
"#,
    )
    .expect("config file should be written");

    let loaded = config::load_config().expect("config should load without telemetry section");

    assert_eq!(loaded.telemetry.service_name, "zyxel-nr5103-monitor");
    assert_eq!(loaded.telemetry.endpoint, None);
    assert_eq!(loaded.telemetry.export_interval, Duration::from_secs(60));
    assert!(!loaded.telemetry.metrics.enabled);
    assert!(!loaded.telemetry.traces.enabled);
    assert!(!loaded.telemetry.logs.enabled);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn config_telemetry_signals_can_be_enabled_independently() {
    let _env_guard = lock_env();

    let cases = [
        (
            "metrics",
            r#"
[telemetry.metrics]
enabled = true
"#,
            true,
            false,
            false,
        ),
        (
            "traces",
            r#"
[telemetry.traces]
enabled = true
"#,
            false,
            true,
            false,
        ),
        (
            "logs",
            r#"
[telemetry.logs]
enabled = true
"#,
            false,
            false,
            true,
        ),
    ];

    for (signal_name, telemetry_config, expected_metrics, expected_traces, expected_logs) in cases {
        let temp_dir = unique_temp_dir();
        let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

        fs::write(
            temp_dir.join("config.toml"),
            format!(
                r#"
[router]
host = "172.16.0.1"
username = "monitor"
password = "secret"

[monitor]
{telemetry_config}
"#,
            ),
        )
        .expect("config file should be written");

        let loaded =
            config::load_config().expect("config should load with explicitly enabled telemetry");

        assert_eq!(
            loaded.telemetry.metrics.enabled, expected_metrics,
            "unexpected metrics enabled state for {signal_name}"
        );
        assert_eq!(
            loaded.telemetry.traces.enabled, expected_traces,
            "unexpected traces enabled state for {signal_name}"
        );
        assert_eq!(
            loaded.telemetry.logs.enabled, expected_logs,
            "unexpected logs enabled state for {signal_name}"
        );

        fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }
}

#[test]
fn config_telemetry_endpoint_and_export_interval_deserialize() {
    let _env_guard = lock_env();
    let temp_dir = unique_temp_dir();
    let _dir_guard = CurrentDirGuard::change_to(&temp_dir);

    fs::write(
        temp_dir.join("config.toml"),
        r#"
[router]
host = "172.16.0.1"
username = "monitor"
password = "secret"

[monitor]

[telemetry]
endpoint = "http://collector.internal:4317"
export_interval = 15
"#,
    )
    .expect("config file should be written");

    let loaded =
        config::load_config().expect("config should load with explicit telemetry settings");

    assert_eq!(
        loaded.telemetry.endpoint.as_deref(),
        Some("http://collector.internal:4317")
    );
    assert_eq!(loaded.telemetry.export_interval, Duration::from_secs(15));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
