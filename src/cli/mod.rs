use std::io;

use clap::{
    ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand,
    error::ErrorKind as ClapErrorKind,
};
use thiserror::Error;

use crate::{
    BINARY_NAME,
    actions::{backup, compact, interactive, restore},
    config::{AppState, ExecutionOptions},
    tmux_adapter::verbose_log::{self, VerboseLogLevel},
};

pub mod catalog_render;
mod observability;
pub mod ui;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Cli(#[from] CliError),
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error(transparent)]
    Backup(#[from] backup::BackupError),
    #[error(transparent)]
    Restore(#[from] restore::RestoreError),
    #[error(transparent)]
    Compact(#[from] compact::CompactError),
    #[error(transparent)]
    Catalog(#[from] crate::storage::CatalogError),
    #[error(transparent)]
    Interactive(#[from] interactive::InteractiveError),
}

const CLI_AFTER_HELP: &str = concat!(
    "Examples:\n",
    "  remux backup\n",
    "  remux backup sprint_demo\n",
    "  remux list\n",
    "  remux restore\n",
    "  remux restore --interactive\n",
    "  remux compact\n",
    "  remux -L sockA backup backup_20240101_120000\n\n",
    "Files:\n",
    "  config file: $HOME/.remux/config.toml"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    List,
    Delete,
    Backup,
    Restore,
    InteractiveRestore,
    Compact,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Delete => "delete",
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::InteractiveRestore => "interactive-restore",
            Self::Compact => "compact",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub socket_name: Option<String>,
    pub verbose_log_level: u8,
    pub action: Action,
    pub action_arg: Option<String>,
}

#[derive(Debug)]
pub struct CliError(clap::Error);

impl CliError {
    pub fn kind(&self) -> ClapErrorKind {
        self.0.kind()
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<clap::Error> for CliError {
    fn from(value: clap::Error) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Parser)]
#[command(
    name = BINARY_NAME,
    version,
    disable_help_subcommand = true,
    subcommand_required = true,
    arg_required_else_help = true,
    propagate_version = true,
    next_line_help = true
)]
struct Cli {
    #[arg(
        short = 'L',
        global = true,
        value_name = "socket-name",
        help = "Use socket-name for the tmux server socket",
        long_help = "Use socket-name for the tmux server socket. The default socket is named default, and a different socket-name allows multiple independent tmux servers."
    )]
    socket_name: Option<String>,

    #[arg(
        short = 'v',
        long = "tmux-verbose",
        global = true,
        action = ArgAction::Count,
        help = "Increase tmux command verbosity"
    )]
    verbose_log_level: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
enum Commands {
    #[command(about = "Capture current tmux sessions")]
    Backup {
        #[arg(value_name = "name", help = "Save using the provided backup name")]
        name: Option<String>,
    },
    #[command(about = "Inspect backups")]
    List {
        #[arg(value_name = "name", help = "Show detailed backup info by name")]
        name: Option<String>,
    },
    #[command(about = "Delete backups")]
    Delete {
        #[arg(value_name = "name", help = "Delete the named backup directly")]
        name: Option<String>,
    },
    #[command(about = "Remove the previous backup when it matches the latest")]
    Compact,
    #[command(about = "Restore tmux sessions from backup")]
    Restore {
        #[arg(value_name = "name", help = "Restore the named backup")]
        name: Option<String>,

        #[arg(
            long,
            help = "Open the interactive restore picker",
            conflicts_with = "name"
        )]
        interactive: bool,
    },
}

pub fn parse_cli_args<I, S>(argv: I) -> Result<CliArgs, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = argv.into_iter().map(Into::into).collect::<Vec<String>>();
    let matches = build_cli().try_get_matches_from(args)?;
    let cli = Cli::from_arg_matches(&matches)?;
    Ok(cli.into_cli_args())
}

pub fn run<I, S>(argv: I) -> AppResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = argv.into_iter().map(Into::into).collect::<Vec<String>>();

    let parsed = match parse_cli_args(args) {
        Ok(parsed) => parsed,
        Err(error)
            if matches!(
                error.kind(),
                ClapErrorKind::DisplayHelp
                    | ClapErrorKind::DisplayVersion
                    | ClapErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) =>
        {
            print!("{error}");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };

    let mut config = AppState::load()?;
    let verbose_log_level = VerboseLogLevel::from_flag_count(parsed.verbose_log_level);
    verbose_log::init(verbose_log_level);
    config.set_execution_options(ExecutionOptions::with_socket_name(
        parsed.socket_name.as_deref(),
    ));

    let action = parsed.action;
    let requested_backup = parsed.action_arg.clone();
    let result = observability::run_with(
        &config,
        action.as_str(),
        requested_backup.as_deref(),
        || {
            tracing::info!(
                action = action.as_str(),
                requested_backup = requested_backup.as_deref().unwrap_or("-"),
                socket_name = config.socket_name().unwrap_or("default"),
                verbose_log_level = ?verbose_log_level,
                "dispatching cli action"
            );
            let result = dispatch(parsed, &config);

            match &result {
                Ok(()) => tracing::info!(action = action.as_str(), "cli action completed"),
                Err(error) => tracing::error!(
                    action = action.as_str(),
                    error = %error,
                    debug_error = ?error,
                    "cli action failed"
                ),
            }

            result
        },
    );

    result
}

pub fn render_error(error: &AppError) -> String {
    match error {
        AppError::Cli(error) => error.to_string().trim_end().to_string(),
        _ => error.to_string(),
    }
}

pub fn usage_text() -> String {
    build_cli().render_long_help().to_string()
}

impl Cli {
    fn into_cli_args(self) -> CliArgs {
        let (action, action_arg) = match self.command {
            Commands::Backup { name } => (Action::Backup, name),
            Commands::List { name } => (Action::List, name),
            Commands::Delete { name } => (Action::Delete, name),
            Commands::Restore {
                name: _,
                interactive: true,
            } => (Action::InteractiveRestore, None),
            Commands::Restore {
                name,
                interactive: false,
            } => (Action::Restore, name),
            Commands::Compact => (Action::Compact, None),
        };

        CliArgs {
            socket_name: self.socket_name,
            verbose_log_level: self.verbose_log_level,
            action,
            action_arg,
        }
    }
}

fn build_cli() -> clap::Command {
    Cli::command()
        .disable_colored_help(true)
        .before_help(ui::brand_block())
        .after_help(CLI_AFTER_HELP)
        .help_template(
            "{before-help}\nUsage: {usage}\n\nCommands:\n{subcommands}\nOptions:\n{options}{after-help}\n",
        )
        .subcommand_help_heading("Commands")
}

fn dispatch(args: CliArgs, config: &AppState) -> AppResult<()> {
    match args.action {
        Action::List => handle_list(config, args.action_arg.as_deref()),
        Action::Delete => match args.action_arg.as_deref() {
            Some(backup_name) => do_delete(config, backup_name),
            None => interactive_delete(config),
        },
        Action::Backup => do_backup(config, args.action_arg.as_deref()),
        Action::Restore => do_restore(config, args.action_arg.as_deref()),
        Action::InteractiveRestore => interactive_restore(config),
        Action::Compact => do_compact(config),
    }
}

fn handle_list(config: &AppState, action_arg: Option<&str>) -> AppResult<()> {
    match action_arg {
        Some(backup_name) => {
            let entry = crate::storage::load_backup(config, backup_name)?;
            println!("{}", catalog_render::render_detail(&entry));
            Ok(())
        }
        None => interactive_list(config),
    }
}

fn do_delete(config: &AppState, backup_name: &str) -> AppResult<()> {
    crate::storage::delete_backup(config, backup_name)?;
    println!("Backup {backup_name} was deleted");
    Ok(())
}

fn do_backup(config: &AppState, action_arg: Option<&str>) -> AppResult<()> {
    if action_arg.is_none() && config.socket_name().is_none() {
        return do_backup_all_sockets(config);
    }

    match backup::capture_backup(config, action_arg) {
        Ok(backup::BackupOutcome::Created { path, .. }) => {
            println!("Backup of sessions was saved under {}", path.display());
            Ok(())
        }
        Ok(backup::BackupOutcome::NoServer) => {
            println!("No tmux session found, nothing to backup");
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// Print every socket that already finished, then fail on the first hard error.
///
/// Later sockets can fail after earlier ones have written snapshots. Those
/// successes must stay visible on stdout so a nonzero exit is not mistaken for
/// "nothing was saved".
fn do_backup_all_sockets(config: &AppState) -> AppResult<()> {
    let results = backup::capture_all_socket_backups(config)?;
    let mut created_backup_count = 0usize;
    let mut first_socket_backup_error = None;

    for result in results {
        record_socket_backup_result(
            result,
            &mut created_backup_count,
            &mut first_socket_backup_error,
        );
    }

    if created_backup_count == 0 && first_socket_backup_error.is_none() {
        println!("No tmux session found, nothing to backup");
    }

    first_socket_backup_error.map_or(Ok(()), |error| Err(error.into()))
}

fn record_socket_backup_result(
    result: backup::SocketBackupResult,
    created_backup_count: &mut usize,
    first_socket_backup_error: &mut Option<backup::BackupError>,
) {
    match result {
        backup::SocketBackupResult::Completed(outcome) => {
            if print_completed_socket_backup(&outcome) {
                *created_backup_count += 1;
            }
        }
        backup::SocketBackupResult::Failed { error, .. } => {
            first_socket_backup_error.get_or_insert(error);
        }
    }
}

/// Returns whether this socket produced a new backup directory.
fn print_completed_socket_backup(outcome: &backup::SocketBackupOutcome) -> bool {
    match &outcome.outcome {
        backup::BackupOutcome::Created { path, .. } => {
            println!(
                "Backup of sessions for socket {} was saved under {}",
                outcome.socket_name,
                path.display()
            );
            true
        }
        backup::BackupOutcome::NoServer => {
            println!(
                "No tmux session found for socket {}, nothing to backup",
                outcome.socket_name
            );
            false
        }
    }
}

fn do_compact(config: &AppState) -> AppResult<()> {
    match compact::compact_latest_pair(config)? {
        compact::CompactOutcome::NeedMoreBackups => {
            println!("Need at least two backups to compact");
        }
        compact::CompactOutcome::NamedPrevious { name } => {
            println!("Previous backup {name} is not an automatic backup");
        }
        compact::CompactOutcome::Different { kept, previous } => {
            println!("Latest backups {kept} and {previous} differ, nothing to compact");
        }
        compact::CompactOutcome::Removed { deleted, kept } => {
            println!("Removed duplicate backup {deleted} (same as {kept})");
        }
    }
    Ok(())
}

fn do_restore(config: &AppState, action_arg: Option<&str>) -> AppResult<()> {
    restore::restore_from_config(config, action_arg)?;
    match action_arg {
        Some(backup_name) => println!("Backup {backup_name} was restored"),
        None => println!("Latest backup was restored"),
    }
    Ok(())
}

fn interactive_list(config: &AppState) -> AppResult<()> {
    with_stdio_locks(|input, output| interactive::interactive_list(config, input, output))
        .map_err(Into::into)
}

fn interactive_delete(config: &AppState) -> AppResult<()> {
    with_stdio_locks(|input, output| interactive::interactive_delete(config, input, output))
        .map_err(Into::into)
}

fn interactive_restore(config: &AppState) -> AppResult<()> {
    with_stdio_locks(|input, output| interactive::interactive_restore(config, input, output))
        .map_err(Into::into)
}

fn with_stdio_locks<F>(run: F) -> Result<(), interactive::InteractiveError>
where
    F: FnOnce(
        &mut io::StdinLock<'_>,
        &mut io::StdoutLock<'_>,
    ) -> Result<(), interactive::InteractiveError>,
{
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    run(&mut input, &mut output)
}
