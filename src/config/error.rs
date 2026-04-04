use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("HOME is not set; cannot resolve ~/.remux")]
    HomeDirNotFound,
    #[error("failed to create {}: {source}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read {}: {source}", path.display())]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse config {}: {source}", path.display())]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("tmux.binary must not be empty")]
    InvalidTmuxBinary,
    #[error("backup.{field} must not be empty: {value:?}")]
    InvalidBackupDirName { field: &'static str, value: String },
    #[error("failed to write {}: {source}", path.display())]
    WriteFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
