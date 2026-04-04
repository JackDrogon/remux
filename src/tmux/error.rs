use std::io;
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SubprocessError {
    #[error(
        "subprocess binary not found for {}: {source}",
        format_command(command)
    )]
    BinaryNotFound {
        command: Vec<String>,
        #[source]
        source: io::Error,
    },
    #[error("failed to spawn subprocess {}: {source}", format_command(command))]
    SpawnFailed {
        command: Vec<String>,
        #[source]
        source: io::Error,
    },
    #[error(
        "failed while waiting for subprocess {}: {source}",
        format_command(command)
    )]
    WaitFailed {
        command: Vec<String>,
        #[source]
        source: io::Error,
    },
    #[error("{}", timed_out_message(.command, *.timeout, stderr))]
    TimedOut {
        command: Vec<String>,
        timeout: Duration,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
    #[error("{}", failed_message(.command, *status, stderr))]
    Failed {
        command: Vec<String>,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

impl SubprocessError {
    pub fn command(&self) -> &[String] {
        match self {
            Self::BinaryNotFound { command, .. }
            | Self::SpawnFailed { command, .. }
            | Self::WaitFailed { command, .. }
            | Self::TimedOut { command, .. }
            | Self::Failed { command, .. } => command,
        }
    }

    pub fn status(&self) -> Option<i32> {
        match self {
            Self::TimedOut { status, .. } | Self::Failed { status, .. } => *status,
            Self::BinaryNotFound { .. } | Self::SpawnFailed { .. } | Self::WaitFailed { .. } => {
                None
            }
        }
    }

    pub fn stdout(&self) -> Option<&str> {
        match self {
            Self::TimedOut { stdout, .. } | Self::Failed { stdout, .. } => Some(stdout),
            Self::BinaryNotFound { .. } | Self::SpawnFailed { .. } | Self::WaitFailed { .. } => {
                None
            }
        }
    }

    pub fn stderr(&self) -> Option<&str> {
        match self {
            Self::TimedOut { stderr, .. } | Self::Failed { stderr, .. } => Some(stderr),
            Self::BinaryNotFound { .. } | Self::SpawnFailed { .. } | Self::WaitFailed { .. } => {
                None
            }
        }
    }
}

fn format_command(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.is_empty() || part.chars().any(char::is_whitespace) {
                format!("{part:?}")
            } else {
                part.clone()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn format_status(status: Option<i32>) -> String {
    status
        .map(|value| value.to_string())
        .unwrap_or_else(|| "signal".to_string())
}

fn timed_out_message(command: &[String], timeout: Duration, stderr: &str) -> String {
    if stderr.is_empty() {
        format!(
            "subprocess timed out after {:?}: {}",
            timeout,
            format_command(command)
        )
    } else {
        format!(
            "subprocess timed out after {:?}: {} (stderr: {})",
            timeout,
            format_command(command),
            stderr
        )
    }
}

fn failed_message(command: &[String], status: Option<i32>, stderr: &str) -> String {
    let status = format_status(status);
    if stderr.is_empty() {
        format!(
            "subprocess exited with status {status}: {}",
            format_command(command)
        )
    } else {
        format!(
            "subprocess exited with status {status}: {} (stderr: {})",
            format_command(command),
            stderr
        )
    }
}
