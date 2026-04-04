use thiserror::Error;

pub use crate::tmux::SubprocessError;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Cli(#[from] crate::cli::CliError),
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error(transparent)]
    Backup(#[from] crate::backup::BackupError),
    #[error(transparent)]
    Restore(#[from] crate::restore::RestoreError),
    #[error(transparent)]
    Catalog(#[from] crate::storage::CatalogError),
    #[error(transparent)]
    Interactive(#[from] crate::interactive::InteractiveError),
}
