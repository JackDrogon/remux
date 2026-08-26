pub mod actions;
pub mod backup_name;
pub mod cli;
pub mod config;
pub mod error;
pub(crate) mod hash;
pub mod model;
pub(crate) mod observability;
pub(crate) mod process;
pub mod storage;
pub mod tmux;
pub mod ui;
pub mod verbose_log;

pub const BINARY_NAME: &str = "remux";
pub const HOME_DIR_NAME: &str = ".remux";
pub const CONFIG_FILE_NAME: &str = "config.toml";

pub fn binary_name() -> &'static str {
    BINARY_NAME
}

pub fn run<I, S>(argv: I) -> Result<(), error::AppError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::run(argv)
}
