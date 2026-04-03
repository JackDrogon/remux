use std::io;

use crate::{
    BINARY_NAME, backup, catalog,
    config::{RuntimeConfig, RuntimeOptions},
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    MissingSocketName,
    MissingAction,
    TooManyArguments,
    UnknownAction(String),
}

impl CliError {
    pub fn message(&self) -> String {
        match self {
            Self::MissingSocketName => "missing socket name for -L".to_string(),
            Self::MissingAction => "missing action".to_string(),
            Self::TooManyArguments => "too many arguments".to_string(),
            Self::UnknownAction(action) => format!("unknown action: {action}"),
        }
    }
}

pub fn parse_cli_args<I, S>(argv: I) -> Result<CliArgs, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut socket_name = None;
    let mut action_flag = None;
    let mut action_arg = None;
    let args = argv.into_iter().map(Into::into).collect::<Vec<String>>();
    let mut idx = 1;

    while idx < args.len() {
        let arg = &args[idx];

        if arg == "-L" {
            let Some(next_arg) = args.get(idx + 1) else {
                return Err(CliError::MissingSocketName);
            };
            if next_arg.is_empty() {
                return Err(CliError::MissingSocketName);
            }
            socket_name = Some(next_arg.clone());
            idx += 2;
            continue;
        }

        if action_flag.is_none() {
            action_flag = Some(arg.clone());
        } else if action_arg.is_none() {
            action_arg = Some(arg.clone());
        } else {
            return Err(CliError::TooManyArguments);
        }

        idx += 1;
    }

    let Some(action_flag) = action_flag else {
        return Err(CliError::MissingAction);
    };

    let Some(action) = Action::from_flag(&action_flag) else {
        return Err(CliError::UnknownAction(action_flag));
    };

    Ok(CliArgs {
        socket_name,
        action,
        action_arg,
    })
}

pub fn run<I, S>(argv: I) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut config = match RuntimeConfig::load() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return 1;
        }
    };

    match parse_cli_args(argv) {
        Ok(args) => {
            config.set_runtime_options(RuntimeOptions::with_socket_name(
                args.socket_name.as_deref(),
            ));
            dispatch(args, &config)
        }
        Err(error) => {
            eprintln!("{}", error.message());
            eprintln!("{}", usage_text());
            1
        }
    }
}

pub fn usage_text() -> String {
    format!(
        "Usage: {BINARY_NAME} [OPTIONS]\n\nOptions:\n  -h                  print help message\n\n  -v                  version\n\n  -l [name]           list backup info\n      with [name]:    show detailed backup info by name\n      without [name]: show brief and detailed info interactively\n\n  -d [name]           delete a backup\n      with [name]:    delete by given name\n      without [name]: delete interactively\n\n  -b [name]           backup current tmux sessions\n      with [name]:    name the backup with given name\n      without [name]: name the backup with default name(timestamp)\n\n  -r [name]           restore tmux sessions from backup\n      with [name]:    restore sessions by backup name\n      without [name]: restore from the latest backup\n\n  -ri                 restore sessions interactively\n  -L [socket-name]    use the given tmux socket name\n  config file: $HOME/.remux/config.toml"
    )
}

fn dispatch(args: CliArgs, config: &RuntimeConfig) -> i32 {
    match args.action {
        Action::Help => {
            println!("{}", usage_text());
            0
        }
        Action::Version => {
            println!("{} {}", BINARY_NAME, env!("CARGO_PKG_VERSION"));
            0
        }
        Action::List => exit_from_result(show_and_action(config, args.action_arg.as_deref())),
        Action::Delete => match args.action_arg.as_deref() {
            Some(_) => exit_from_result(do_delete(config, args.action_arg.as_deref())),
            None => exit_from_result(interactive_delete(config)),
        },
        Action::Backup => exit_from_result(do_backup(config, args.action_arg.as_deref())),
        Action::Restore => exit_from_result(do_restore(config, args.action_arg.as_deref())),
        Action::InteractiveRestore => exit_from_result(interactive_restore(config)),
    }
}

fn exit_from_result(result: Result<(), String>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn show_and_action(config: &RuntimeConfig, action_arg: Option<&str>) -> Result<(), String> {
    match action_arg {
        Some(backup_name) => {
            let entry =
                catalog::load_backup(config, backup_name).map_err(|error| error.to_string())?;
            println!("{}", catalog::render_detail(&entry));
            Ok(())
        }
        None => interactive_list(config),
    }
}

fn do_delete(config: &RuntimeConfig, action_arg: Option<&str>) -> Result<(), String> {
    let backup_name = action_arg.ok_or_else(|| "delete requires a backup name".to_string())?;
    catalog::delete_backup(config, backup_name).map_err(|error| error.to_string())?;
    println!("Backup {backup_name} was deleted");
    Ok(())
}

fn do_backup(config: &RuntimeConfig, action_arg: Option<&str>) -> Result<(), String> {
    match backup::capture_backup(config, action_arg) {
        Ok(backup::BackupOutcome::Created { path, .. }) => {
            println!("Backup of sessions was saved under {}", path.display());
            Ok(())
        }
        Ok(backup::BackupOutcome::NoServer) => {
            println!("No tmux session found, nothing to backup");
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn do_restore(config: &RuntimeConfig, action_arg: Option<&str>) -> Result<(), String> {
    restore::restore_from_config(config, action_arg)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn interactive_list(config: &RuntimeConfig) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    interactive::interactive_list(config, &mut input, &mut output)
}

fn interactive_delete(config: &RuntimeConfig) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    interactive::interactive_delete(config, &mut input, &mut output)
}

fn interactive_restore(config: &RuntimeConfig) -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    interactive::interactive_restore(config, &mut input, &mut output)
}
