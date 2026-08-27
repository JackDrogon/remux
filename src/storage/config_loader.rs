use crate::config::{AppConfig, ConfigError, ConfigPaths};

use super::fs_ops;

pub const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../assets/config.toml");

pub fn load_or_init_app_config(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
    bootstrap_config(paths)?;
    let config = parse_config_file(paths)?;
    ensure_runtime_dirs(paths, &config)?;
    Ok(config)
}

fn bootstrap_config(paths: &ConfigPaths) -> Result<(), ConfigError> {
    if paths.config_file.exists() {
        return Ok(());
    }

    fs_ops::create_dir_all(&paths.user_path, |path, source| ConfigError::CreateDir {
        path,
        source,
    })?;
    fs_ops::write_string(
        &paths.config_file,
        DEFAULT_CONFIG_TEMPLATE,
        |path, source| ConfigError::WriteFile { path, source },
    )?;

    Ok(())
}

fn parse_config_file(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
    let content = fs_ops::read_to_string(&paths.config_file, |path, source| {
        ConfigError::ReadFile { path, source }
    })?;
    let config: AppConfig = toml::from_str(&content).map_err(|source| ConfigError::ParseToml {
        path: paths.config_file.clone(),
        source,
    })?;
    config.validate()?;
    Ok(config)
}

fn ensure_runtime_dirs(paths: &ConfigPaths, config: &AppConfig) -> Result<(), ConfigError> {
    let backup_root = paths.backup_root(config);
    fs_ops::create_dir_all(&backup_root, |path, source| ConfigError::CreateDir {
        path,
        source,
    })
}
