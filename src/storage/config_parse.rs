use crate::config::{AppConfig, ConfigError, ConfigPaths};

use super::fs_ops;

pub(super) fn parse_config_file(paths: &ConfigPaths) -> Result<AppConfig, ConfigError> {
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
