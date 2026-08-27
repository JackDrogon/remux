pub mod actions;
pub mod cli;
pub mod config;
pub mod model;
pub mod storage;
pub mod tmux_adapter;

pub const BINARY_NAME: &str = "remux";
pub const HOME_DIR_NAME: &str = ".remux";
pub const CONFIG_FILE_NAME: &str = "config.toml";

pub fn run<I, S>(argv: I) -> Result<(), cli::AppError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    cli::run(argv)
}
