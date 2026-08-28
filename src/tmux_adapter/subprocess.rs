use std::io;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use super::verbose_log::{self, VerboseLogLevel};
use crate::{Error, Result, Tmux as TmuxError};

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
    fn execute(&self, command: Vec<String>) -> Result<CommandOutput>;
    fn execute_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput>;
}

/// Runs tmux as a child and waits until it exits.
///
/// remux is a synchronous CLI: a hung tmux is a hung remux. There is no
/// adapter-level deadline or `TmuxTimedOut`; adding one without draining
/// pipes would mis-timeout `capture-pane`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubprocessRunner;

impl SubprocessRunner {
    fn run(&self, command: Vec<String>) -> Result<CommandOutput> {
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
        let output = match wait_for_output(command.clone(), child) {
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

    fn run_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput> {
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
        let output = match wait_for_output(command.clone(), child) {
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
    fn execute(&self, command: Vec<String>) -> Result<CommandOutput> {
        self.run(command)
    }

    fn execute_bytes(&self, command: Vec<String>) -> Result<ByteCommandOutput> {
        self.run_bytes(command)
    }
}

fn spawn_command(command: &[String]) -> Result<Child> {
    let Some(program) = command.first() else {
        return Err(TmuxError::SpawnFailed {
            command: Vec::new(),
            source: io::Error::other("empty command"),
        }
        .into());
    };
    Command::new(program)
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| {
            match source.kind() {
                std::io::ErrorKind::NotFound => TmuxError::BinaryNotFound {
                    command: command.to_vec(),
                    source,
                },
                _ => TmuxError::SpawnFailed {
                    command: command.to_vec(),
                    source,
                },
            }
            .into()
        })
}

fn wait_for_output(command: Vec<String>, child: Child) -> Result<Output> {
    // Unbounded on purpose: stdout/stderr stay piped into this wait, so the
    // child cannot stall on a full pipe. A timeout loop that only `try_wait`s
    // would deadlock large captures.
    child
        .wait_with_output()
        .map_err(|source| TmuxError::WaitFailed { command, source }.into())
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

fn log_subprocess_error(command: &[String], error: &Error, elapsed: Duration, byte_stream: bool) {
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
    error: &Error,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_returns_typed_spawn_failure() {
        let error = SubprocessRunner
            .execute(Vec::new())
            .expect_err("empty argv must not panic");
        assert!(matches!(
            error.code(),
            crate::Code::Tmux(TmuxError::SpawnFailed { command, .. }) if command.is_empty()
        ));
    }
}
