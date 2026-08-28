use std::io::{self, Write};

use crate::{
    BINARY_NAME, Error, Result,
    actions::{backup, compact, interactive, restore},
    config::{AppState, ExecutionOptions},
    error::Cli as CliError,
    tmux_adapter::verbose_log::{self, VerboseLogLevel},
};
use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};

pub mod catalog_render;
mod observability;
pub mod ui;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    List { name: Option<String> },
    Delete { name: Option<String> },
    Backup { name: Option<String> },
    Restore { name: Option<String> },
    InteractiveRestore,
    Compact,
}

impl CliCommand {
    fn action_name(&self) -> &'static str {
        match self {
            Self::List { .. } => "list",
            Self::Delete { .. } => "delete",
            Self::Backup { .. } => "backup",
            Self::Restore { .. } => "restore",
            Self::InteractiveRestore => "interactive-restore",
            Self::Compact => "compact",
        }
    }

    fn requested_backup(&self) -> Option<&str> {
        match self {
            Self::List { name }
            | Self::Delete { name }
            | Self::Backup { name }
            | Self::Restore { name } => name.as_deref(),
            Self::InteractiveRestore | Self::Compact => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub socket_name: Option<String>,
    pub verbose_log_level: u8,
    pub command: CliCommand,
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
    #[command(about = "Remove the previous backup when it is covered by the latest")]
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

pub fn parse_cli_args<I, S>(argv: I) -> std::result::Result<CliArgs, clap::Error>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = argv.into_iter().map(Into::into).collect::<Vec<String>>();
    let matches = build_cli().try_get_matches_from(args)?;
    let cli = Cli::from_arg_matches(&matches)?;
    Ok(cli.into_cli_args())
}

fn exit_clap(error: clap::Error) -> ! {
    let _ = error.print();
    std::process::exit(error.exit_code());
}

pub fn run<I, S>(argv: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = argv.into_iter().map(Into::into).collect::<Vec<String>>();

    let parsed = match parse_cli_args(args) {
        Ok(parsed) => parsed,
        Err(error) => exit_clap(error),
    };

    let mut config = AppState::load()?;
    let verbose_log_level = VerboseLogLevel::from_flag_count(parsed.verbose_log_level);
    verbose_log::init(verbose_log_level);
    config.set_execution_options(ExecutionOptions::with_socket_name(
        parsed.socket_name.as_deref(),
    ));

    let action = parsed.command.action_name();
    let requested_backup = parsed.command.requested_backup().map(str::to_string);
    observability::run_with(&config, action, requested_backup.as_deref(), || {
        tracing::info!(
            action,
            requested_backup = requested_backup.as_deref().unwrap_or("-"),
            socket_name = config.socket_name().unwrap_or("default"),
            verbose_log_level = ?verbose_log_level,
            "dispatching cli action"
        );
        let result = dispatch(parsed, &config);

        match &result {
            Ok(()) => tracing::info!(action, "cli action completed"),
            // Tracing is the operational log (file, and console only when the
            // user enabled `[logging]`). `render_error` in `main` is the single
            // user-facing terminal report. These are different channels, not a
            // second walk of the error chain.
            Err(error) => tracing::error!(
                action,
                error = %error,
                debug_error = ?error,
                "cli action failed"
            ),
        }

        result
    })
}

pub fn render_error(error: &Error) -> String {
    error.to_string()
}

pub fn usage_text() -> String {
    build_cli().render_long_help().to_string()
}

impl Cli {
    fn into_cli_args(self) -> CliArgs {
        let command = match self.command {
            Commands::Backup { name } => CliCommand::Backup { name },
            Commands::List { name } => CliCommand::List { name },
            Commands::Delete { name } => CliCommand::Delete { name },
            Commands::Restore {
                name: _,
                interactive: true,
            } => CliCommand::InteractiveRestore,
            Commands::Restore {
                name,
                interactive: false,
            } => CliCommand::Restore { name },
            Commands::Compact => CliCommand::Compact,
        };

        CliArgs {
            socket_name: self.socket_name,
            verbose_log_level: self.verbose_log_level,
            command,
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

fn dispatch(args: CliArgs, config: &AppState) -> Result<()> {
    match args.command {
        CliCommand::List { name } => handle_list(config, name.as_deref()),
        CliCommand::Delete { name } => match name.as_deref() {
            Some(backup_name) => do_delete(config, backup_name),
            None => interactive_delete(config),
        },
        CliCommand::Backup { name } => do_backup(config, name.as_deref()),
        CliCommand::Restore { name } => do_restore(config, name.as_deref()),
        CliCommand::InteractiveRestore => interactive_restore(config),
        CliCommand::Compact => do_compact(config),
    }
}

fn handle_list(config: &AppState, action_arg: Option<&str>) -> Result<()> {
    match action_arg {
        Some(backup_name) => {
            let entry = crate::storage::load_backup(config, backup_name)?;
            emit_line(catalog_render::render_detail(&entry))
        }
        None => interactive_list(config),
    }
}

fn do_delete(config: &AppState, backup_name: &str) -> Result<()> {
    crate::storage::delete_backup(config, backup_name)?;
    emit_line(format!("Backup {backup_name} was deleted"))
}

fn do_backup(config: &AppState, action_arg: Option<&str>) -> Result<()> {
    if action_arg.is_none() && config.socket_name().is_none() {
        return do_backup_all_sockets(config);
    }

    match backup::capture_backup(config, action_arg) {
        Ok(backup::BackupOutcome::Created { path, .. }) => emit_line(format!(
            "Backup of sessions was saved under {}",
            path.display()
        )),
        Ok(backup::BackupOutcome::NoServer) => {
            emit_line("No tmux session found, nothing to backup")
        }
        Err(error) => Err(error),
    }
}

/// Print every socket that already finished, then fail on the first hard error.
///
/// Later sockets can fail after earlier ones have written snapshots. Those
/// successes must stay visible on stdout so a nonzero exit is not mistaken for
/// "nothing was saved".
fn do_backup_all_sockets(config: &AppState) -> Result<()> {
    let results = backup::capture_all_socket_backups(config)?;
    let mut created_backup_count = 0usize;
    let mut first_socket_backup_error = None;

    for result in results {
        record_socket_backup_result(
            result,
            &mut created_backup_count,
            &mut first_socket_backup_error,
        )?;
    }

    if created_backup_count == 0 && first_socket_backup_error.is_none() {
        emit_line("No tmux session found, nothing to backup")?;
    }

    first_socket_backup_error.map_or(Ok(()), Err)
}

fn record_socket_backup_result(
    result: backup::SocketBackupResult,
    created_backup_count: &mut usize,
    first_socket_backup_error: &mut Option<Error>,
) -> Result<()> {
    match result {
        backup::SocketBackupResult::Completed(outcome) => {
            if print_completed_socket_backup(&outcome)? {
                *created_backup_count += 1;
            }
        }
        backup::SocketBackupResult::Failed { error, .. } => {
            if first_socket_backup_error.is_none() {
                *first_socket_backup_error = Some(error);
            }
        }
    }
    Ok(())
}

/// Returns whether this socket produced a new backup directory.
fn print_completed_socket_backup(outcome: &backup::SocketBackupOutcome) -> Result<bool> {
    match &outcome.outcome {
        backup::BackupOutcome::Created { path, .. } => {
            emit_line(format!(
                "Backup of sessions for socket {} was saved under {}",
                outcome.socket_name,
                path.display()
            ))?;
            Ok(true)
        }
        backup::BackupOutcome::NoServer => {
            emit_line(format!(
                "No tmux session found for socket {}, nothing to backup",
                outcome.socket_name
            ))?;
            Ok(false)
        }
    }
}

fn do_compact(config: &AppState) -> Result<()> {
    match compact::compact_latest_pair(config)? {
        compact::CompactOutcome::NeedMoreBackups => {
            emit_line("Need at least two backups to compact")?;
        }
        compact::CompactOutcome::NamedPrevious { name } => {
            emit_line(format!("Previous backup {name} is not an automatic backup"))?;
        }
        compact::CompactOutcome::Different { kept, previous } => {
            emit_line(format!(
                "Latest backups {kept} and {previous} differ, nothing to compact"
            ))?;
        }
        compact::CompactOutcome::Removed { deleted, kept } => {
            emit_line(format!(
                "Removed backup {deleted} (covered by {kept})"
            ))?;
        }
    }
    Ok(())
}

fn do_restore(config: &AppState, action_arg: Option<&str>) -> Result<()> {
    restore::restore_from_config(config, action_arg)?;
    match action_arg {
        Some(backup_name) => emit_line(format!("Backup {backup_name} was restored")),
        None => emit_line("Latest backup was restored"),
    }
}

fn interactive_list(config: &AppState) -> Result<()> {
    with_stdio_locks(|input, output| interactive::interactive_list(config, input, output))
}

fn interactive_delete(config: &AppState) -> Result<()> {
    with_stdio_locks(|input, output| interactive::interactive_delete(config, input, output))
}

fn interactive_restore(config: &AppState) -> Result<()> {
    with_stdio_locks(|input, output| interactive::interactive_restore(config, input, output))
}

fn with_stdio_locks<F>(run: F) -> Result<()>
where
    F: FnOnce(&mut io::StdinLock<'_>, &mut io::StdoutLock<'_>) -> Result<()>,
{
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    run(&mut input, &mut output)
}

fn emit_line(text: impl std::fmt::Display) -> Result<()> {
    write_stdout(text, true)
}

/// Non-interactive CLI reports. BrokenPipe means the peer is gone: success.
fn write_stdout(text: impl std::fmt::Display, newline: bool) -> Result<()> {
    let mut stdout = io::stdout();
    let written = if newline {
        writeln!(stdout, "{text}")
    } else {
        write!(stdout, "{text}")
    };
    map_noninteractive_stdout(written)
}

fn map_noninteractive_stdout(written: io::Result<()>) -> Result<()> {
    match written {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(CliError::Stdout(error).into()),
    }
}

#[cfg(test)]
mod stdout_contract_tests {
    use super::*;
    use crate::{Category, Code};

    #[test]
    fn broken_pipe_is_success() {
        let error = io::Error::new(io::ErrorKind::BrokenPipe, "peer gone");
        map_noninteractive_stdout(Err(error)).expect("BrokenPipe is success for CLI reports");
    }

    #[test]
    fn other_write_failures_are_cli_stdout() {
        let error = io::Error::other("disk full");
        let err = map_noninteractive_stdout(Err(error)).expect_err("other writes fail");
        assert_eq!(err.category(), Category::Cli);
        assert!(matches!(err.code(), Code::Cli(CliError::Stdout(_))));
    }
}
