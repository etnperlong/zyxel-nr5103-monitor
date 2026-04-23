use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use zyxel_nr5103_monitor::config;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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
    let _env_guard = env_lock().lock().expect("env lock should not be poisoned");
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
    assert_eq!(loaded.monitor.interval, Duration::from_secs(60));
    assert_eq!(loaded.monitor.url, "http://www.gstatic.com/generate_204");
    assert_eq!(loaded.monitor.timeout, Duration::from_secs(5));
    assert_eq!(loaded.monitor.max_retries, 1);
    assert_eq!(loaded.monitor.min_reboot_interval, Duration::from_secs(300));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
