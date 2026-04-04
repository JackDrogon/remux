pub mod backup;
pub mod backup_name;
pub mod cli;
pub mod config;
pub mod error;
pub(crate) mod hash;
pub mod interactive;
pub mod model;
pub mod restore;
pub mod storage;
pub mod tmux;

pub const BINARY_NAME: &str = "remux";
pub const HOME_DIR_NAME: &str = ".remux";
pub const CONFIG_FILE_NAME: &str = "config.toml";

pub fn binary_name() -> &'static str {
    BINARY_NAME
}

pub fn run<I, S>(argv: I) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::run(argv)
}
