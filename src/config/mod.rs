mod error;
mod loader;
mod paths;
mod runtime;
mod schema;

pub use error::ConfigError;
pub use loader::DEFAULT_CONFIG_TEMPLATE;
pub use paths::{ConfigPaths, sanitize_socket_name, socket_dir_name};
pub use runtime::{RuntimeConfig, RuntimeContext, RuntimeOptions};
pub use schema::{AppConfig, BackupConfig, CaptureConfig, LogLevel, LoggingConfig, TmuxConfig};
