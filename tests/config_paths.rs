use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use retmux::config::{ConfigPaths, DEFAULT_CONFIG_TEMPLATE, RuntimeConfig, socket_dir_name};

#[test]
fn default_socket_uses_legacy_backup_root() {
    let temp_home = TempHome::new("default-socket");
    let config = RuntimeConfig::load_from_home(temp_home.path())
        .expect("default config should bootstrap and load");

    assert_eq!(config.socket_name(), None);
    assert_eq!(
        config.active_backup_path(),
        config.paths().backup_root.as_path()
    );
    assert_eq!(
        config.active_backup_path(),
        temp_home.path().join(".retmux/backup").as_path()
    );
    assert_eq!(config.tmux_cmd_prefix(), &[String::from("tmux")]);
}

#[test]
fn named_socket_uses_sanitized_backup_root() {
    let temp_home = TempHome::new("named-socket");
    let mut config = RuntimeConfig::load_from_home(temp_home.path())
        .expect("default config should bootstrap and load");

    config.activate_socket(Some("custom/socket name"));

    assert_eq!(
        socket_dir_name(Some("custom/socket name")).as_deref(),
        Some("custom_socket_name")
    );
    assert_eq!(config.socket_name(), Some("custom/socket name"));
    assert_eq!(
        config.active_backup_path(),
        temp_home
            .path()
            .join(".retmux/backup-sockets/custom_socket_name")
            .as_path()
    );
    assert_eq!(
        config.tmux_cmd_prefix(),
        &[
            String::from("tmux"),
            String::from("-L"),
            String::from("custom/socket name"),
        ]
    );
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

    assert!(paths.user_path.is_dir(), "expected ~/.retmux to be created");
    assert!(
        paths.backup_root.is_dir(),
        "expected backup root to be created"
    );
    assert!(
        paths.config_file.is_file(),
        "expected retmux.conf to be created"
    );
    assert_eq!(
        fs::read_to_string(&paths.config_file).expect("bootstrapped config should be readable"),
        DEFAULT_CONFIG_TEMPLATE
    );
    assert!(
        config.content_with_escape,
        "legacy default should keep escapes enabled"
    );
}

#[test]
fn malformed_config_is_reported() {
    let temp_home = TempHome::new("malformed");
    let paths = ConfigPaths::from_home(temp_home.path());
    fs::create_dir_all(&paths.user_path).expect("should create ~/.retmux for malformed test");
    fs::write(
        &paths.config_file,
        "[settings]\nlog.level.file = INFO\nlog.level.console = INFO\ncontent.with.escape = maybe\n",
    )
    .expect("should write malformed config");

    let error = RuntimeConfig::load_from_paths(paths).unwrap_err();
    let message = error.to_string();

    assert!(
        message.contains("content.with.escape") && message.contains("invalid boolean"),
        "unexpected malformed-config error: {message}"
    );
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
            "retmux-config-paths-{label}-{}-{unique}",
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
