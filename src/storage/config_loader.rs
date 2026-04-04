use crate::config::{AppConfig, ConfigError, ConfigPaths};

use super::fs_ops;

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

    create_dir_all(&paths.user_path)?;
    write_file(&paths.config_file, DEFAULT_CONFIG_TEMPLATE)?;

    Ok(())
}

fn parse_config_file(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
    let content = read_to_string(&paths.config_file)?;
    let config: AppConfig = toml::from_str(&content).map_err(|source| ConfigError::ParseToml {
        path: paths.config_file.clone(),
        source,
    })?;
    config.validate()?;
    Ok(config)
}

fn ensure_runtime_dirs(paths: &ConfigPaths, config: &AppConfig) -> Result<(), ConfigError> {
    let backup_root = paths.backup_root(config);
    create_dir_all(&backup_root)
}

fn create_dir_all(path: &std::path::Path) -> Result<(), ConfigError> {
    fs_ops::create_dir_all(path, |path, source| ConfigError::CreateDir { path, source })
}

fn write_file(path: &std::path::Path, content: &str) -> Result<(), ConfigError> {
    fs_ops::write_string(path, content, |path, source| ConfigError::WriteFile {
        path,
        source,
    })
}

fn read_to_string(path: &std::path::Path) -> Result<String, ConfigError> {
    fs_ops::read_to_string(path, |path, source| ConfigError::ReadFile { path, source })
}
