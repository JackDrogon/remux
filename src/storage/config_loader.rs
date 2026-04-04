use crate::config::{AppConfig, ConfigError, ConfigPaths};

use super::config_bootstrap::bootstrap_config;
use super::config_parse::parse_config_file;
use super::fs_ops;

pub const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../assets/config.toml");

pub fn load_or_init_app_config(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
    bootstrap_config(paths)?;
    let app = parse_config_file(paths)?;
    ensure_runtime_dirs(paths, &app)?;
    Ok(app)
}

fn ensure_runtime_dirs(paths: &ConfigPaths, config: &AppConfig) -> Result<(), ConfigError> {
    let backup_root = paths.backup_root(config);
    fs_ops::create_dir_all(&backup_root, |path, source| ConfigError::CreateDir {
        path,
        source,
    })
}
