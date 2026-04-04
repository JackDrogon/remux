use crate::config::{ConfigError, ConfigPaths};

use super::config_loader::DEFAULT_CONFIG_TEMPLATE;
use super::fs_ops;

pub(super) fn bootstrap_config(paths: &ConfigPaths) -> Result<(), ConfigError> {
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
