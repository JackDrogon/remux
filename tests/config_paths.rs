use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::config::{
    AppConfig, ConfigPaths, DEFAULT_CONFIG_TEMPLATE, RuntimeConfig, RuntimeOptions, socket_dir_name,
};

#[test]
fn default_socket_uses_legacy_backup_root() {
    let temp_home = TempHome::new("default-socket");
    let config = RuntimeConfig::load_from_home(temp_home.path())
        .expect("default config should bootstrap and load");

    assert_eq!(config.socket_name(), None);
    assert_eq!(
        config.active_backup_path(),
        config.paths().backup_root(config.app())
    );
    assert_eq!(
        config.active_backup_path(),
        temp_home.path().join(".remux/backup")
    );
    assert_eq!(config.tmux_command_prefix(), vec![String::from("tmux")]);
}

#[test]
fn named_socket_uses_sanitized_backup_root() {
    let temp_home = TempHome::new("named-socket");
    let mut config = RuntimeConfig::load_from_home(temp_home.path())
        .expect("default config should bootstrap and load");

    config.set_runtime_options(RuntimeOptions::with_socket_name(Some("custom/socket name")));

    assert_eq!(
        socket_dir_name(Some("custom/socket name")).as_deref(),
        Some("custom_socket_name")
    );
    assert_eq!(config.socket_name(), Some("custom/socket name"));
    assert_eq!(
        config.active_backup_path(),
        temp_home
            .path()
            .join(".remux/backup-sockets/custom_socket_name")
    );
    assert_eq!(
        config.tmux_command_prefix(),
        vec![
            String::from("tmux"),
            String::from("-L"),
            String::from("custom/socket name"),
        ]
    );
}

#[test]
fn runtime_options_recompute_socket_dependent_values() {
    let temp_home = TempHome::new("runtime-options");
    let mut config = RuntimeConfig::load_from_home(temp_home.path())
        .expect("default config should bootstrap and load");

    config.set_runtime_options(RuntimeOptions::with_socket_name(Some("sock/A")));
    assert_eq!(config.socket_name(), Some("sock/A"));
    assert_eq!(
        config.active_backup_path(),
        temp_home.path().join(".remux/backup-sockets/sock_A")
    );
    assert_eq!(
        config.tmux_command_prefix(),
        vec![
            String::from("tmux"),
            String::from("-L"),
            String::from("sock/A"),
        ]
    );

    config.set_runtime_options(RuntimeOptions::default());
    assert_eq!(config.socket_name(), None);
    assert_eq!(
        config.active_backup_path(),
        temp_home.path().join(".remux/backup")
    );
    assert_eq!(config.tmux_command_prefix(), vec![String::from("tmux")]);
}

#[test]
fn missing_config_is_bootstrapped() {
    let temp_home = TempHome::new("bootstrap");
    let paths = ConfigPaths::from_home(temp_home.path());
    assert!(
        !paths.config_file.exists(),
        "bootstrap fixture should start empty"
    );

    let config = RuntimeConfig::load_from_paths(paths.clone())
        .expect("missing config should be bootstrapped and loaded");

    assert!(paths.user_path.is_dir(), "expected ~/.remux to be created");
    assert!(
        paths.backup_root(config.app()).is_dir(),
        "expected backup root to be created"
    );
    assert!(
        paths.config_file.is_file(),
        "expected config.toml to be created"
    );
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("bootstrapped config should be readable"),
        DEFAULT_CONFIG_TEMPLATE
    );
    assert!(
        config.content_with_escape(),
        "default config should keep escapes enabled"
    );
}

#[test]
fn malformed_config_is_reported() {
    let temp_home = TempHome::new("malformed");
    let paths = ConfigPaths::from_home(temp_home.path());
    fs::create_dir_all(&paths.user_path).expect("should create ~/.remux for malformed test");
    fs::write(
        &paths.config_file,
        "[logging]\nfile = \"info\"\nconsole = \"info\"\n\n[capture]\nwith_escape = \"maybe\"\n",
    )
    .expect("should write malformed config");

    let error = RuntimeConfig::load_from_paths(paths).unwrap_err();
    let message = error.to_string();

    assert!(
        message.contains("with_escape") && message.contains("boolean"),
        "unexpected malformed-config error: {message}"
    );
}

#[test]
fn default_app_config_exposes_readable_sections() {
    let config = AppConfig::default();

    assert_eq!(config.logging.file.as_str(), "info");
    assert_eq!(config.logging.console.as_str(), "info");
    assert_eq!(config.tmux.binary, "tmux");
    assert_eq!(config.backup.dir_name, "backup");
    assert_eq!(config.backup.socket_dir_name, "backup-sockets");
}

struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-config-paths-{label}-{}-{unique}",
            std::process::id()
        ));

        if path.exists() {
            fs::remove_dir_all(&path).expect("should clear stale temp HOME");
        }
        fs::create_dir_all(&path).expect("should create temp HOME");

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
