use std::fmt;
use std::io;
use std::time::Duration;

#[derive(Debug)]
pub enum SubprocessError {
    BinaryNotFound {
        command: Vec<String>,
        source: io::Error,
    },
    SpawnFailed {
        command: Vec<String>,
        source: io::Error,
    },
    WaitFailed {
        command: Vec<String>,
        source: io::Error,
    },
    TimedOut {
        command: Vec<String>,
        timeout: Duration,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
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

impl fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryNotFound { command, source } => write!(
                f,
                "subprocess binary not found for {}: {source}",
                format_command(command)
            ),
            Self::SpawnFailed { command, source } => write!(
                f,
                "failed to spawn subprocess {}: {source}",
                format_command(command)
            ),
            Self::WaitFailed { command, source } => write!(
                f,
                "failed while waiting for subprocess {}: {source}",
                format_command(command)
            ),
            Self::TimedOut {
                command,
                timeout,
                stderr,
                ..
            } => {
                if stderr.is_empty() {
                    write!(
                        f,
                        "subprocess timed out after {:?}: {}",
                        timeout,
                        format_command(command)
                    )
                } else {
                    write!(
                        f,
                        "subprocess timed out after {:?}: {} (stderr: {})",
                        timeout,
                        format_command(command),
                        stderr
                    )
                }
            }
            Self::Failed {
                command,
                status,
                stderr,
                ..
            } => {
                let status = format_status(*status);
                if stderr.is_empty() {
                    write!(
                        f,
                        "subprocess exited with status {status}: {}",
                        format_command(command)
                    )
                } else {
                    write!(
                        f,
                        "subprocess exited with status {status}: {} (stderr: {})",
                        format_command(command),
                        stderr
                    )
                }
            }
        }
    }
}

impl std::error::Error for SubprocessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::BinaryNotFound { source, .. }
            | Self::SpawnFailed { source, .. }
            | Self::WaitFailed { source, .. } => Some(source),
            Self::TimedOut { .. } | Self::Failed { .. } => None,
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
