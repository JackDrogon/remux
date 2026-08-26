use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::error::SubprocessError;
use crate::verbose_log::{self, VerboseLogLevel};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
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
        let started_at = Instant::now();
        log_tmux_command_start(&command);
        let child = match spawn_command(&command) {
            Ok(child) => child,
            Err(error) => {
                log_tmux_command_failure(&command, started_at.elapsed(), &error, false);
                log_subprocess_error(&command, &error, started_at.elapsed(), false);
                return Err(error);
            }
        };
        let output = match wait_for_output(command.clone(), child, self.timeout) {
            Ok(output) => output,
            Err(error) => {
                log_tmux_command_failure(&command, started_at.elapsed(), &error, false);
                log_subprocess_error(&command, &error, started_at.elapsed(), false);
                return Err(error);
            }
        };

        let output = CommandOutput {
            command,
            status: output.status.code(),
            stdout: normalize_output_stream(output.stdout),
            stderr: normalize_output_stream(output.stderr),
        };
        log_tmux_command_finish(
            &output.command,
            output.status,
            started_at.elapsed(),
            output.stdout.len(),
            output.stderr.len(),
            false,
        );
        log_command_output(&output, started_at.elapsed(), false);
        Ok(output)
    }

    fn run_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput, SubprocessError> {
        let started_at = Instant::now();
        log_tmux_command_start(&command);
        let child = match spawn_command(&command) {
            Ok(child) => child,
            Err(error) => {
                log_tmux_command_failure(&command, started_at.elapsed(), &error, true);
                log_subprocess_error(&command, &error, started_at.elapsed(), true);
                return Err(error);
            }
        };
        let output = match wait_for_output(command.clone(), child, self.timeout) {
            Ok(output) => output,
            Err(error) => {
                log_tmux_command_failure(&command, started_at.elapsed(), &error, true);
                log_subprocess_error(&command, &error, started_at.elapsed(), true);
                return Err(error);
            }
        };

        let output = ByteCommandOutput {
            command,
            status: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        };
        log_tmux_command_finish(
            &output.command,
            output.status,
            started_at.elapsed(),
            output.stdout.len(),
            output.stderr.len(),
            true,
        );
        log_byte_command_output(&output, started_at.elapsed());
        Ok(output)
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
        // Blocking `wait_with_output` cannot honor a deadline, so poll until
        // the process exits or the timeout elapses.
        if !subprocess_exited_before_deadline(&command, &mut child, timeout)? {
            return kill_timed_out_subprocess(command, child, timeout);
        }
    }

    child
        .wait_with_output()
        .map_err(|source| SubprocessError::WaitFailed { command, source })
}

fn subprocess_exited_before_deadline(
    command: &[String],
    child: &mut Child,
    timeout: Duration,
) -> Result<bool, SubprocessError> {
    let deadline = Instant::now() + timeout;
    loop {
        if subprocess_has_exited(command, child)? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn subprocess_has_exited(command: &[String], child: &mut Child) -> Result<bool, SubprocessError> {
    match child.try_wait() {
        Ok(status) => Ok(status.is_some()),
        Err(source) => Err(SubprocessError::WaitFailed {
            command: command.to_vec(),
            source,
        }),
    }
}

/// Kill the still-running process and keep its output for the timeout error.
///
/// `wait_with_output` has to take ownership of `Child` to drain stdout/stderr,
/// so this path consumes the process instead of returning it to the caller.
fn kill_timed_out_subprocess(
    command: Vec<String>,
    mut child: Child,
    timeout: Duration,
) -> Result<Output, SubprocessError> {
    let _ = child.kill();
    let output = child
        .wait_with_output()
        .map_err(|source| SubprocessError::WaitFailed {
            command: command.clone(),
            source,
        })?;

    Err(SubprocessError::TimedOut {
        command,
        timeout,
        status: output.status.code(),
        stdout: normalize_output_stream(output.stdout),
        stderr: normalize_output_stream(output.stderr),
    })
}

fn log_command_output(output: &CommandOutput, elapsed: Duration, byte_stream: bool) {
    tracing::info!(
        command = %format_command_for_log(&output.command),
        status_code = ?output.status,
        elapsed_ms = elapsed.as_millis() as u64,
        stdout_len = output.stdout.len(),
        stderr_len = output.stderr.len(),
        byte_stream,
        "tmux subprocess finished"
    );
}

fn log_byte_command_output(output: &ByteCommandOutput, elapsed: Duration) {
    tracing::info!(
        command = %format_command_for_log(&output.command),
        status_code = ?output.status,
        elapsed_ms = elapsed.as_millis() as u64,
        stdout_len = output.stdout.len(),
        stderr_len = output.stderr.len(),
        byte_stream = true,
        "tmux subprocess finished"
    );
}

fn log_subprocess_error(
    command: &[String],
    error: &SubprocessError,
    elapsed: Duration,
    byte_stream: bool,
) {
    tracing::error!(
        command = %format_command_for_log(command),
        error = %error,
        debug_error = ?error,
        elapsed_ms = elapsed.as_millis() as u64,
        byte_stream,
        "tmux subprocess failed"
    );
}

fn format_command_for_log(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.is_empty() || part.chars().any(char::is_whitespace) {
                format!("{part:?}")
            } else {
                part.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn log_tmux_command_start(command: &[String]) {
    let rendered_command = format_command_for_log(command);
    verbose_log::log(
        VerboseLogLevel::Debug1,
        format_args!("debug1: executing tmux command: {rendered_command}\n"),
    );
}

fn log_tmux_command_finish(
    command: &[String],
    status_code: Option<i32>,
    elapsed: Duration,
    stdout_len: usize,
    stderr_len: usize,
    byte_stream: bool,
) {
    let rendered_command = format_command_for_log(command);
    verbose_log::log(
        VerboseLogLevel::Debug2,
        format_args!(
            "debug2: tmux command finished: {rendered_command} status={status_code:?} elapsed_ms={} stdout_len={stdout_len} stderr_len={stderr_len} byte_stream={byte_stream}\n",
            elapsed.as_millis() as u64,
        ),
    );
}

fn log_tmux_command_failure(
    command: &[String],
    elapsed: Duration,
    error: &SubprocessError,
    byte_stream: bool,
) {
    let rendered_command = format_command_for_log(command);
    verbose_log::log(
        VerboseLogLevel::Debug2,
        format_args!(
            "debug2: tmux command failed: {rendered_command} elapsed_ms={} byte_stream={byte_stream} error={error}\n",
            elapsed.as_millis() as u64,
        ),
    );
}

pub(crate) fn normalize_output_stream(bytes: Vec<u8>) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.ends_with('\n') {
        text.pop();
    }
    text
}
