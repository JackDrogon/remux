use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::{LEGACY_CONFIG_FILE, LEGACY_HOME_DIR};

pub const LEGACY_BACKUP_DIR: &str = "backup";
pub const LEGACY_BACKUP_SOCKET_DIR: &str = "backup-sockets";
pub const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../assets/retmux.default.conf");

const DEFAULT_LOG_LEVEL: &str = "INFO";
const SETTINGS_SECTION: &str = "settings";
const TMUX_BIN: &str = "tmux";
const VALID_LOG_LEVELS: [&str; 3] = ["info", "debug", "error"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPaths {
    pub user_path: PathBuf,
    pub backup_root: PathBuf,
    pub backup_socket_root: PathBuf,
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
        let user_path = home_dir.as_ref().join(LEGACY_HOME_DIR);
        let backup_root = user_path.join(LEGACY_BACKUP_DIR);
        let backup_socket_root = user_path.join(LEGACY_BACKUP_SOCKET_DIR);
        let config_file = user_path.join(LEGACY_CONFIG_FILE);

        Self {
            user_path,
            backup_root,
            backup_socket_root,
            config_file,
        }
    }

    pub fn active_backup_path(&self, socket_name: Option<&str>) -> PathBuf {
        match socket_dir_name(socket_name) {
            Some(socket_dir_name) => self.backup_socket_root.join(socket_dir_name),
            None => self.backup_root.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    paths: ConfigPaths,
    socket_name: Option<String>,
    active_backup_path: PathBuf,
    tmux_cmd_prefix: Vec<String>,
    pub content_with_escape: bool,
    pub log_level_file: String,
    pub log_level_console: String,
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
        let parsed = parse_config_file(&paths.config_file)?;

        Ok(Self {
            active_backup_path: paths.active_backup_path(None),
            tmux_cmd_prefix: vec![TMUX_BIN.to_string()],
            paths,
            socket_name: None,
            content_with_escape: parsed.content_with_escape,
            log_level_file: parsed.log_level_file,
            log_level_console: parsed.log_level_console,
        })
    }

    pub fn activate_socket(&mut self, socket_name: Option<&str>) {
        self.socket_name = socket_name
            .map(str::trim)
            .filter(|socket_name| !socket_name.is_empty())
            .map(ToOwned::to_owned);

        self.tmux_cmd_prefix = vec![TMUX_BIN.to_string()];
        if let Some(socket_name) = self.socket_name.as_deref() {
            self.tmux_cmd_prefix.push("-L".to_string());
            self.tmux_cmd_prefix.push(socket_name.to_string());
        }

        self.active_backup_path = self.paths.active_backup_path(self.socket_name.as_deref());
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn socket_name(&self) -> Option<&str> {
        self.socket_name.as_deref()
    }

    pub fn active_backup_path(&self) -> &Path {
        &self.active_backup_path
    }

    pub fn tmux_cmd_prefix(&self) -> &[String] {
        &self.tmux_cmd_prefix
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
    WriteFile {
        path: PathBuf,
        source: io::Error,
    },
    MissingSection {
        path: PathBuf,
        section: &'static str,
    },
    MissingKey {
        path: PathBuf,
        key: &'static str,
    },
    InvalidBoolean {
        path: PathBuf,
        key: &'static str,
        value: String,
    },
    InvalidLine {
        path: PathBuf,
        line_number: usize,
        line: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeDirNotFound => write!(f, "HOME is not set; cannot resolve ~/.retmux"),
            Self::CreateDir { path, source } => {
                write!(f, "failed to create {}: {source}", path.display())
            }
            Self::ReadFile { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            Self::WriteFile { path, source } => {
                write!(f, "failed to write {}: {source}", path.display())
            }
            Self::MissingSection { path, section } => write!(
                f,
                "config {} is missing required section [{section}]",
                path.display()
            ),
            Self::MissingKey { path, key } => {
                write!(f, "config {} is missing required key {key}", path.display())
            }
            Self::InvalidBoolean { path, key, value } => write!(
                f,
                "config {} has invalid boolean for {key}: {value}",
                path.display()
            ),
            Self::InvalidLine {
                path,
                line_number,
                line,
            } => write!(
                f,
                "config {} has invalid line {line_number}: {line}",
                path.display()
            ),
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

    fs::create_dir_all(&paths.backup_root).map_err(|source| ConfigError::CreateDir {
        path: paths.backup_root.clone(),
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

fn parse_config_file(path: &Path) -> Result<ParsedSettings, ConfigError> {
    let content = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let mut current_section: Option<&str> = None;
    let mut saw_settings_section = false;
    let mut settings = HashMap::new();

    for (index, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if let Some(section_name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let section_name = section_name.trim();
            if section_name.is_empty() {
                return Err(ConfigError::InvalidLine {
                    path: path.to_path_buf(),
                    line_number: index + 1,
                    line: raw_line.to_string(),
                });
            }

            current_section = Some(section_name);
            saw_settings_section |= section_name == SETTINGS_SECTION;
            continue;
        }

        let Some(section_name) = current_section else {
            return Err(ConfigError::InvalidLine {
                path: path.to_path_buf(),
                line_number: index + 1,
                line: raw_line.to_string(),
            });
        };

        let Some((raw_key, raw_value)) = line.split_once('=') else {
            return Err(ConfigError::InvalidLine {
                path: path.to_path_buf(),
                line_number: index + 1,
                line: raw_line.to_string(),
            });
        };

        if section_name == SETTINGS_SECTION {
            settings.insert(raw_key.trim().to_string(), raw_value.trim().to_string());
        }
    }

    if !saw_settings_section {
        return Err(ConfigError::MissingSection {
            path: path.to_path_buf(),
            section: SETTINGS_SECTION,
        });
    }

    let log_level_file = parse_log_level(setting_value(&settings, path, "log.level.file")?);
    let log_level_console = parse_log_level(setting_value(&settings, path, "log.level.console")?);
    let content_with_escape = parse_boolean(
        path,
        "content.with.escape",
        setting_value(&settings, path, "content.with.escape")?,
    )?;

    Ok(ParsedSettings {
        log_level_file,
        log_level_console,
        content_with_escape,
    })
}

fn setting_value<'a>(
    settings: &'a HashMap<String, String>,
    path: &Path,
    key: &'static str,
) -> Result<&'a str, ConfigError> {
    settings
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| ConfigError::MissingKey {
            path: path.to_path_buf(),
            key,
        })
}

fn parse_log_level(value: &str) -> String {
    if VALID_LOG_LEVELS.contains(&value.to_ascii_lowercase().as_str()) {
        value.to_string()
    } else {
        DEFAULT_LOG_LEVEL.to_string()
    }
}

fn parse_boolean(path: &Path, key: &'static str, value: &str) -> Result<bool, ConfigError> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::InvalidBoolean {
            path: path.to_path_buf(),
            key,
            value: value.to_string(),
        }),
    }
}

#[derive(Debug)]
struct ParsedSettings {
    log_level_file: String,
    log_level_console: String,
    content_with_escape: bool,
}
