use std::env;
use std::path::{Path, PathBuf};

use crate::{CONFIG_FILE_NAME, HOME_DIR_NAME};

use super::{AppConfig, ConfigError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPaths {
    pub user_path: PathBuf,
    pub config_file: PathBuf,
}

impl ConfigPaths {
    pub fn from_env() -> Result<Self, ConfigError> {
        let Some(home_dir) = env::var_os("HOME") else {
            return Err(ConfigError::HomeDirNotFound);
        };

        Ok(Self::from_home(PathBuf::from(home_dir)))
    }

    pub fn from_home<P>(home_dir: P) -> Self
    where
        P: AsRef<Path>,
    {
        let user_path = home_dir.as_ref().join(HOME_DIR_NAME);
        let config_file = user_path.join(CONFIG_FILE_NAME);

        Self {
            user_path,
            config_file,
        }
    }

    pub fn backup_root(&self, config: &AppConfig) -> PathBuf {
        self.user_path.join(&config.backup.dir_name)
    }

    pub fn backup_socket_root(&self, config: &AppConfig) -> PathBuf {
        self.user_path.join(&config.backup.socket_dir_name)
    }

    pub fn active_backup_path(&self, config: &AppConfig, socket_name: Option<&str>) -> PathBuf {
        match socket_dir_name(socket_name) {
            Some(socket_dir_name) => self.backup_socket_root(config).join(socket_dir_name),
            None => self.backup_root(config),
        }
    }
}

pub fn socket_dir_name(socket_name: Option<&str>) -> Option<String> {
    socket_name
        .map(str::trim)
        .filter(|socket_name| !socket_name.is_empty())
        .map(sanitize_socket_name)
}

pub fn sanitize_socket_name(socket_name: &str) -> String {
    socket_name
        .chars()
        .map(|ch| match ch {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '.' | '-' => ch,
            _ => '_',
        })
        .collect()
}
