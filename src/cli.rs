use std::io;

use thiserror::Error;

use crate::{
    BINARY_NAME, backup,
    config::{AppState, ExecutionOptions},
    error::{AppError, AppResult},
    interactive, restore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Help,
    Version,
    List,
    Delete,
    Backup,
    Restore,
    InteractiveRestore,
}

impl Action {
    pub fn from_flag(flag: &str) -> Option<Self> {
        match flag {
            "-h" => Some(Self::Help),
            "-v" => Some(Self::Version),
            "-l" => Some(Self::List),
            "-d" => Some(Self::Delete),
            "-b" => Some(Self::Backup),
            "-r" => Some(Self::Restore),
            "-ri" => Some(Self::InteractiveRestore),
            _ => None,
        }
    }

    pub fn flag(self) -> &'static str {
        match self {
            Self::Help => "-h",
            Self::Version => "-v",
            Self::List => "-l",
            Self::Delete => "-d",
            Self::Backup => "-b",
            Self::Restore => "-r",
            Self::InteractiveRestore => "-ri",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliArgs {
    pub socket_name: Option<String>,
    pub action: Action,
    pub action_arg: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CliError {
    #[error("missing socket name for -L")]
    MissingSocketName,
    #[error("missing action")]
    MissingAction,
    #[error("too many arguments")]
    TooManyArguments,
    #[error("unknown action: {0}")]
    UnknownAction(String),
}

pub fn parse_cli_args<I, S>(argv: I) -> Result<CliArgs, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = argv.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut state = CliArgsState::default();
    let mut idx = 1;

    while idx < args.len() {
        let arg = &args[idx];

        if arg == "-L" {
            state.socket_name = read_socket_name(&args, idx)?;
            idx += 2;
            continue;
        }

        state.push_positional(arg)?;

        idx += 1;
    }

    state.finish()
}

pub fn run<I, S>(argv: I) -> AppResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut config = AppState::load()?;
    let args = parse_cli_args(argv)?;
    config.set_execution_options(ExecutionOptions::with_socket_name(
        args.socket_name.as_deref(),
    ));
    dispatch(args, &config)
}

pub fn render_error(error: &AppError) -> String {
    match error {
        AppError::Cli(error) => format!("{error}\n{}", usage_text()),
        _ => error.to_string(),
    }
}

pub fn usage_text() -> String {
    format!(
        "Usage: {BINARY_NAME} [OPTIONS]\n\nOptions:\n  -h                  print help message\n\n  -v                  version\n\n  -l [name]           list backup info\n      with [name]:    show detailed backup info by name\n      without [name]: show brief and detailed info interactively\n\n  -d [name]           delete a backup\n      with [name]:    delete by given name\n      without [name]: delete interactively\n\n  -b [name]           backup current tmux sessions\n      with [name]:    name the backup with given name\n      without [name]: name the backup with default name(timestamp)\n\n  -r [name]           restore tmux sessions from backup\n      with [name]:    restore sessions by backup name\n      without [name]: restore from the latest backup\n\n  -ri                 restore sessions interactively\n  -L [socket-name]    use the given tmux socket name\n  config file: $HOME/.remux/config.toml"
    )
}

fn dispatch(args: CliArgs, config: &AppState) -> AppResult<()> {
    match args.action {
        Action::Help => {
            println!("{}", usage_text());
            Ok(())
        }
        Action::Version => {
            println!("{} {}", BINARY_NAME, env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Action::List => handle_list(config, args.action_arg.as_deref()),
        Action::Delete => match args.action_arg.as_deref() {
            Some(backup_name) => do_delete(config, backup_name),
            None => interactive_delete(config),
        },
        Action::Backup => do_backup(config, args.action_arg.as_deref()),
        Action::Restore => do_restore(config, args.action_arg.as_deref()),
        Action::InteractiveRestore => interactive_restore(config),
    }
}

fn handle_list(config: &AppState, action_arg: Option<&str>) -> AppResult<()> {
    match action_arg {
        Some(backup_name) => {
            let entry = crate::storage::load_backup(config, backup_name)?;
            println!("{}", crate::storage::render_detail(&entry));
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

fn do_restore(config: &AppState, action_arg: Option<&str>) -> AppResult<()> {
    restore::restore_from_config(config, action_arg)?;
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

#[derive(Default)]
struct CliArgsState {
    socket_name: Option<String>,
    action_flag: Option<String>,
    action_arg: Option<String>,
}

impl CliArgsState {
    fn push_positional(&mut self, arg: &str) -> Result<(), CliError> {
        if self.action_flag.is_none() {
            self.action_flag = Some(arg.to_string());
        } else if self.action_arg.is_none() {
            self.action_arg = Some(arg.to_string());
        } else {
            return Err(CliError::TooManyArguments);
        }

        Ok(())
    }

    fn finish(self) -> Result<CliArgs, CliError> {
        let action_flag = self.action_flag.ok_or(CliError::MissingAction)?;
        let action = Action::from_flag(&action_flag).ok_or(CliError::UnknownAction(action_flag))?;

        Ok(CliArgs {
            socket_name: self.socket_name,
            action,
            action_arg: self.action_arg,
        })
    }
}

fn read_socket_name(args: &[String], index: usize) -> Result<Option<String>, CliError> {
    let next_arg = args.get(index + 1).ok_or(CliError::MissingSocketName)?;
    if next_arg.is_empty() {
        return Err(CliError::MissingSocketName);
    }

    Ok(Some(next_arg.clone()))
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
