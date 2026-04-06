use serde::{Deserialize, Serialize};

use super::ConfigError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub logging: LoggingConfig,
    pub capture: CaptureConfig,
    pub tmux: TmuxConfig,
    pub backup: BackupConfig,
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_tmux_binary(&self.tmux.binary)?;

        validate_backup_dir_name("dir_name", &self.backup.dir_name)?;
        validate_backup_dir_name("socket_dir_name", &self.backup.socket_dir_name)?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub file: LogLevel,
    pub console: LogLevel,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            file: LogLevel::Info,
            console: LogLevel::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Info,
    Debug,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CaptureConfig {
    pub with_escape: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self { with_escape: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TmuxConfig {
    pub binary: String,
}

impl Default for TmuxConfig {
    fn default() -> Self {
        Self {
            binary: "tmux".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BackupConfig {
    pub dir_name: String,
    pub socket_dir_name: String,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            dir_name: "backup".to_string(),
            socket_dir_name: "backup-sockets".to_string(),
        }
    }
}

fn validate_tmux_binary(binary: &str) -> Result<(), ConfigError> {
    if binary.trim().is_empty() {
        return Err(ConfigError::InvalidTmuxBinary);
    }

    Ok(())
}

fn validate_backup_dir_name(field: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.trim().is_empty() {
        return Err(ConfigError::InvalidBackupDirName {
            field,
            value: value.to_string(),
        });
    }

    Ok(())
}
