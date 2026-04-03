use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{CONFIG_FILE_NAME, HOME_DIR_NAME};

pub const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../assets/config.toml");

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOptions {
    socket_name: Option<String>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self { socket_name: None }
    }
}

impl RuntimeOptions {
    pub fn with_socket_name(socket_name: Option<&str>) -> Self {
        Self {
            socket_name: socket_name
                .map(str::trim)
                .filter(|socket_name| !socket_name.is_empty())
                .map(ToOwned::to_owned),
        }
    }

    pub fn socket_name(&self) -> Option<&str> {
        self.socket_name.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppConfig {
    pub logging: LoggingConfig,
    pub capture: CaptureConfig,
    pub tmux: TmuxConfig,
    pub backup: BackupConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            logging: LoggingConfig::default(),
            capture: CaptureConfig::default(),
            tmux: TmuxConfig::default(),
            backup: BackupConfig::default(),
        }
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
            console: LogLevel::Info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Debug,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    paths: ConfigPaths,
    app: AppConfig,
    runtime: RuntimeOptions,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeContext<'a> {
    paths: &'a ConfigPaths,
    app: &'a AppConfig,
    runtime: &'a RuntimeOptions,
}

impl RuntimeConfig {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_paths(ConfigPaths::from_env()?)
    }

    pub fn load_from_home<P>(home_dir: P) -> Result<Self, ConfigError>
    where
        P: AsRef<Path>,
    {
        Self::load_from_paths(ConfigPaths::from_home(home_dir))
    }

    pub fn load_from_paths(paths: ConfigPaths) -> Result<Self, ConfigError> {
        bootstrap_config(&paths)?;
        let app = parse_config_file(&paths.config_file)?;
        ensure_runtime_dirs(&paths, &app)?;

        Ok(Self {
            paths,
            app,
            runtime: RuntimeOptions::default(),
        })
    }

    pub fn set_runtime_options(&mut self, runtime: RuntimeOptions) {
        self.runtime = runtime;
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn app(&self) -> &AppConfig {
        &self.app
    }

    pub fn runtime_options(&self) -> &RuntimeOptions {
        &self.runtime
    }

    pub fn runtime_context(&self) -> RuntimeContext<'_> {
        RuntimeContext {
            paths: &self.paths,
            app: &self.app,
            runtime: &self.runtime,
        }
    }

    pub fn socket_name(&self) -> Option<&str> {
        self.runtime.socket_name()
    }

    pub fn active_backup_path(&self) -> PathBuf {
        self.runtime_context().active_backup_path()
    }

    pub fn tmux_command_prefix(&self) -> Vec<String> {
        self.runtime_context().tmux_command_prefix()
    }

    pub fn content_with_escape(&self) -> bool {
        self.app.capture.with_escape
    }

    pub fn log_level_file(&self) -> &'static str {
        self.app.logging.file.as_str()
    }

    pub fn log_level_console(&self) -> &'static str {
        self.app.logging.console.as_str()
    }
}

impl RuntimeContext<'_> {
    pub fn socket_name(&self) -> Option<&str> {
        self.runtime.socket_name()
    }

    pub fn active_backup_path(&self) -> PathBuf {
        self.paths
            .active_backup_path(self.app, self.runtime.socket_name())
    }

    pub fn tmux_command_prefix(&self) -> Vec<String> {
        let mut prefix = vec![self.app.tmux.binary.clone()];
        if let Some(socket_name) = self.socket_name() {
            prefix.push("-L".to_string());
            prefix.push(socket_name.to_string());
        }
        prefix
    }
}

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

fn parse_config_file(path: &Path) -> Result<AppConfig, ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let config: AppConfig = toml::from_str(&content).map_err(|source| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source,
    })?;
    validate_config(&config)?;
    Ok(config)
}

fn ensure_runtime_dirs(paths: &ConfigPaths, config: &AppConfig) -> Result<(), ConfigError> {
    let backup_root = paths.backup_root(config);
    fs::create_dir_all(&backup_root).map_err(|source| ConfigError::CreateDir {
        path: backup_root,
        source,
    })
}

fn validate_config(config: &AppConfig) -> Result<(), ConfigError> {
    if config.tmux.binary.trim().is_empty() {
        return Err(ConfigError::InvalidTmuxBinary);
    }

    validate_backup_dir_name("dir_name", &config.backup.dir_name)?;
    validate_backup_dir_name("socket_dir_name", &config.backup.socket_dir_name)?;

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
