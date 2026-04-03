use std::path::PathBuf;

use super::{AppConfig, ConfigError, ConfigPaths, loader};

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
        P: AsRef<std::path::Path>,
    {
        Self::load_from_paths(ConfigPaths::from_home(home_dir))
    }

    pub fn load_from_paths(paths: ConfigPaths) -> Result<Self, ConfigError> {
        let app = loader::load_or_init_app_config(&paths)?;

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
