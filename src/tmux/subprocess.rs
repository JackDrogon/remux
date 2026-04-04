use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::error::SubprocessError;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub command: Vec<String>,
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteCommandOutput {
    pub command: Vec<String>,
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl ByteCommandOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

pub trait SubprocessExecutor {
    fn execute(&self, command: Vec<String>) -> Result<CommandOutput, SubprocessError>;
    fn execute_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput, SubprocessError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubprocessRunner {
    timeout: Option<Duration>,
}

impl SubprocessRunner {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn run(&self, command: Vec<String>) -> Result<CommandOutput, SubprocessError> {
        let child = spawn_command(&command)?;
        let output = wait_for_output(command.clone(), child, self.timeout)?;

        Ok(CommandOutput {
            command,
            status: output.status.code(),
            stdout: normalize_output_stream(output.stdout),
            stderr: normalize_output_stream(output.stderr),
        })
    }

    fn run_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput, SubprocessError> {
        let child = spawn_command(&command)?;
        let output = wait_for_output(command.clone(), child, self.timeout)?;

        Ok(ByteCommandOutput {
            command,
            status: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

impl SubprocessExecutor for SubprocessRunner {
    fn execute(&self, command: Vec<String>) -> Result<CommandOutput, SubprocessError> {
        self.run(command)
    }

    fn execute_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput, SubprocessError> {
        self.run_bytes(command)
    }
}

fn spawn_command(command: &[String]) -> Result<Child, SubprocessError> {
    Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => SubprocessError::BinaryNotFound {
                command: command.to_vec(),
                source,
            },
            _ => SubprocessError::SpawnFailed {
                command: command.to_vec(),
                source,
            },
        })
}

fn wait_for_output(
    command: Vec<String>,
    mut child: Child,
    timeout: Option<Duration>,
) -> Result<Output, SubprocessError> {
    if let Some(timeout) = timeout {
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let output = child.wait_with_output().map_err(|source| {
                            SubprocessError::WaitFailed {
                                command: command.clone(),
                                source,
                            }
                        })?;

                        return Err(SubprocessError::TimedOut {
                            command,
                            timeout,
                            status: output.status.code(),
                            stdout: normalize_output_stream(output.stdout),
                            stderr: normalize_output_stream(output.stderr),
                        });
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(source) => {
                    return Err(SubprocessError::WaitFailed { command, source });
                }
            }
        }
    }

    child
        .wait_with_output()
        .map_err(|source| SubprocessError::WaitFailed { command, source })
}

pub(crate) fn normalize_output_stream(bytes: Vec<u8>) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.ends_with('\n') {
        text.pop();
    }
    text
}
