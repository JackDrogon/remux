pub mod actions;
pub mod cli;
pub mod config;
pub mod error;
pub mod model;
pub mod storage;
pub mod tmux_adapter;

pub use error::{
    Backup, Catalog, Category, Cli, Code, Config, Error, Interactive, Restore, Result, Snapshot,
    Tmux,
};

pub const BINARY_NAME: &str = "remux";
pub const HOME_DIR_NAME: &str = ".remux";
pub const CONFIG_FILE_NAME: &str = "config.toml";

pub fn run<I, S>(argv: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::run(argv)
}
