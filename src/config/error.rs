use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ConfigError {
    HomeDirNotFound,
    CreateDir {
        path: PathBuf,
        source: io::Error,
    },
    ReadFile {
        path: PathBuf,
        source: io::Error,
    },
    ParseToml {
        path: PathBuf,
        source: toml::de::Error,
    },
    InvalidTmuxBinary,
    InvalidBackupDirName {
        field: &'static str,
        value: String,
    },
    WriteFile {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeDirNotFound => write!(f, "HOME is not set; cannot resolve ~/.remux"),
            Self::CreateDir { path, source } => {
                write!(f, "failed to create {}: {source}", path.display())
            }
            Self::ReadFile { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            Self::ParseToml { path, source } => {
                write!(f, "failed to parse config {}: {source}", path.display())
            }
            Self::InvalidTmuxBinary => write!(f, "tmux.binary must not be empty"),
            Self::InvalidBackupDirName { field, value } => {
                write!(f, "backup.{field} must not be empty: {value:?}")
            }
            Self::WriteFile { path, source } => {
                write!(f, "failed to write {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConfigError {}
