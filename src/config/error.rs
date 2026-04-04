use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

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
                display_path_error(f, "failed to create", path, source)
            }
            Self::ReadFile { path, source } => {
                display_path_error(f, "failed to read", path, source)
            }
            Self::ParseToml { path, source } => {
                write!(f, "failed to parse config {}: {source}", path.display())
            }
            Self::InvalidTmuxBinary => write!(f, "tmux.binary must not be empty"),
            Self::InvalidBackupDirName { field, value } => {
                write!(f, "backup.{field} must not be empty: {value:?}")
            }
            Self::WriteFile { path, source } => {
                display_path_error(f, "failed to write", path, source)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

fn display_path_error(
    f: &mut fmt::Formatter<'_>,
    action: &str,
    path: &Path,
    source: &io::Error,
) -> fmt::Result {
    write!(f, "{action} {}: {source}", path.display())
}
