pub mod backup;
pub mod backup_name;
pub mod catalog;
pub mod cli;
pub mod config;
pub mod error;
pub mod interactive;
pub mod model;
pub mod restore;
pub mod serde_legacy;
pub mod tmux;

pub const BINARY_NAME: &str = "retmux";
pub const LEGACY_HOME_DIR: &str = ".retmux";
pub const LEGACY_CONFIG_FILE: &str = "retmux.conf";

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
