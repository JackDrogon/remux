use std::fs;

use super::{AppConfig, ConfigError, ConfigPaths};

pub const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../../assets/config.toml");

pub fn load_or_init_app_config(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
    bootstrap_config(paths)?;
    let app = parse_config_file(paths)?;
    ensure_runtime_dirs(paths, &app)?;
    Ok(app)
}

fn bootstrap_config(paths: &ConfigPaths) -> Result<(), ConfigError> {
    if paths.config_file.exists() {
        return Ok(());
    }

    fs::create_dir_all(&paths.user_path).map_err(|source| ConfigError::CreateDir {
        path: paths.user_path.clone(),
        source,
    })?;
    fs::write(&paths.config_file, DEFAULT_CONFIG_TEMPLATE).map_err(|source| {
        ConfigError::WriteFile {
            path: paths.config_file.clone(),
            source,
        }
    })?;

    Ok(())
}

fn parse_config_file(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
    let content =
        fs::read_to_string(&paths.config_file).map_err(|source| ConfigError::ReadFile {
            path: paths.config_file.clone(),
            source,
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
    fs::create_dir_all(&backup_root).map_err(|source| ConfigError::CreateDir {
        path: backup_root,
        source,
    })
}
