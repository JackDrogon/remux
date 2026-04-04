//! Runtime-facing configuration accessors.
//!
//! `AppState` owns both the parsed static config and the transient execution
//! options that come from CLI flags. Centralizing the derived accessors here is
//! what keeps backup, catalog, and restore flows consistent when `-L` switches
//! the active tmux socket and therefore the active backup root.

use std::path::PathBuf;

use super::{paths::normalize_socket_name, AppConfig, ConfigError, ConfigPaths};
use crate::storage;
use crate::tmux::tmux_command_prefix;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionOptions {
    socket_name: Option<String>,
}

impl ExecutionOptions {
    pub fn with_socket_name(socket_name: Option<&str>) -> Self {
        Self {
            socket_name: normalize_socket_name(socket_name).map(ToOwned::to_owned),
        }
    }

    pub fn socket_name(&self) -> Option<&str> {
        self.socket_name.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    paths: ConfigPaths,
    config: AppConfig,
    execution: ExecutionOptions,
}

#[derive(Debug, Clone, Copy)]
pub struct ExecutionContext<'a> {
    paths: &'a ConfigPaths,
    config: &'a AppConfig,
    execution: &'a ExecutionOptions,
}

impl AppState {
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from_paths(ConfigPaths::from_env()?)
    }

    pub fn load_from_home<P>(home_dir: P) -> Result<Self, ConfigError>
    where
        P: AsRef<std::path::Path>,
    {
        Self::load_from_paths(ConfigPaths::from_home(home_dir))
    }

    pub fn load_from_paths(paths: ConfigPaths) -> Result<Self, ConfigError> {
        let config = storage::load_or_init_app_config(&paths)?;

        Ok(Self {
            paths,
            config,
            execution: ExecutionOptions::default(),
        })
    }

    pub fn set_execution_options(&mut self, execution: ExecutionOptions) {
        self.execution = execution;
    }

    pub fn paths(&self) -> &ConfigPaths {
        &self.paths
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    fn execution_context(&self) -> ExecutionContext<'_> {
        ExecutionContext {
            paths: &self.paths,
            config: &self.config,
            execution: &self.execution,
        }
    }

    pub fn socket_name(&self) -> Option<&str> {
        self.execution.socket_name()
    }

    pub fn active_backup_path(&self) -> PathBuf {
        self.execution_context().active_backup_path()
    }

    pub fn tmux_command_prefix(&self) -> Vec<String> {
        self.execution_context().tmux_command_prefix()
    }
}

impl ExecutionContext<'_> {
    pub fn socket_name(&self) -> Option<&str> {
        self.execution.socket_name()
    }

    pub fn active_backup_path(&self) -> PathBuf {
        self.paths
            .active_backup_path(self.config, self.execution.socket_name())
    }

    pub fn tmux_command_prefix(&self) -> Vec<String> {
        tmux_command_prefix(&self.config.tmux.binary, self.socket_name())
    }
}
