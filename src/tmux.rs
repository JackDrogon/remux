use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::AppState;
use crate::error::SubprocessError;

pub const TMUX_BIN: &str = "tmux";
pub const OUTPUT_SEPARATOR: &str = ":=:";
pub const LIST_SESSIONS_FORMAT: &str =
    "#S:=:(#{window_width},#{window_height}):=:#{session_attached}";
pub const LIST_PANES_FORMAT: &str =
    "#{pane_index}:=:(#{pane_width},#{pane_height}):=:#{pane_current_path}:=:#{pane_active}";
pub const LIST_WINDOWS_FORMAT: &str =
    "#{window_index}:=:#{window_name}:=:#{window_active}:=:#{window_layout}";

const CAPTURE_HISTORY_START: &str = "-S-100000";
const SPLIT_WINDOW_SIZE: &str = "-l3";
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
pub struct TmuxAdapter {
    command_prefix: Vec<String>,
    content_with_escape: bool,
    timeout: Option<Duration>,
}

impl TmuxAdapter {
    pub fn new(config: &AppState) -> Self {
        Self {
            command_prefix: config.tmux_command_prefix(),
            content_with_escape: config.config().capture.with_escape,
            timeout: None,
        }
    }

    pub fn from_prefix(command_prefix: Vec<String>, content_with_escape: bool) -> Self {
        Self {
            command_prefix,
            content_with_escape,
            timeout: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn command_prefix(&self) -> &[String] {
        &self.command_prefix
    }

    pub fn render_command(&self, command: TmuxCommand) -> Vec<String> {
        let parts = command.into_parts();
        if parts.first().map(|part| part.as_str()) == Some(TMUX_BIN) {
            let mut rendered = self.command_prefix.clone();
            rendered.extend(parts.into_iter().skip(1));
            rendered
        } else {
            parts
        }
    }

    pub fn run_raw(&self, command: TmuxCommand) -> Result<CommandOutput, SubprocessError> {
        let command = self.render_command(command);
        execute_command(command, self.timeout)
    }

    pub fn run(&self, command: TmuxCommand) -> Result<CommandOutput, SubprocessError> {
        let output = self.run_raw(command)?;
        if output.success() {
            Ok(output)
        } else {
            Err(SubprocessError::Failed {
                command: output.command,
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
    }

    pub fn has_server(&self) -> Result<bool, SubprocessError> {
        Ok(self.run_raw(TmuxCommand::ListSessions)?.success())
    }

    pub fn list_sessions(&self) -> Result<Vec<String>, SubprocessError> {
        Ok(split_legacy_lines(
            &self.run(TmuxCommand::ListSessions)?.stdout,
        ))
    }

    pub fn list_windows(&self, session_name: &str) -> Result<Vec<String>, SubprocessError> {
        Ok(split_legacy_lines(
            &self
                .run(TmuxCommand::ListWindows {
                    session_name: session_name.to_string(),
                })?
                .stdout,
        ))
    }

    pub fn list_panes(
        &self,
        session_name: &str,
        window_index: usize,
    ) -> Result<Vec<String>, SubprocessError> {
        Ok(split_legacy_lines(
            &self
                .run(TmuxCommand::ListPanes {
                    session_name: session_name.to_string(),
                    window_index,
                })?
                .stdout,
        ))
    }

    pub fn create_session(
        &self,
        session_name: &str,
        width: u32,
        height: u32,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::CreateSession {
            session_name: session_name.to_string(),
            width,
            height,
        })?;
        Ok(())
    }

    pub fn kill_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        Ok(self
            .run_raw(TmuxCommand::KillSession {
                session_name: session_name.to_string(),
            })?
            .success())
    }

    pub fn capture_pane(&self, pane_id: &str) -> Result<String, SubprocessError> {
        Ok(self
            .run(TmuxCommand::CapturePane {
                pane_id: pane_id.to_string(),
                include_escape: self.content_with_escape,
            })?
            .stdout)
    }

    pub fn show_option(&self, option: &str) -> Result<String, SubprocessError> {
        Ok(self
            .run(TmuxCommand::ShowOption {
                option: option.to_string(),
            })?
            .stdout)
    }

    pub fn has_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        Ok(self
            .run_raw(TmuxCommand::HasSession {
                session_name: session_name.to_string(),
            })?
            .success())
    }

    pub fn clear_pane(&self, pane_id: &str) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::ClearPane {
            pane_id: pane_id.to_string(),
        })?;
        Ok(())
    }

    pub fn send_keys(&self, target: &str, keys: &str) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::SendKeys {
            target: target.to_string(),
            keys: keys.to_string(),
        })?;
        Ok(())
    }

    pub fn set_pane_path(&self, pane_id: &str, path: &Path) -> Result<(), SubprocessError> {
        let path = path.to_string_lossy();
        self.clear_pane(pane_id)?;
        self.send_keys(pane_id, &format!("builtin cd \"{path}\"\nclear\n"))?;
        self.clear_pane(pane_id)?;
        Ok(())
    }

    pub fn create_empty_window(
        &self,
        session_name: &str,
        base_index: usize,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::NewEmptyWindow {
            session_name: session_name.to_string(),
            base_index,
        })?;
        Ok(())
    }

    pub fn move_window(&self, source: &str, target: &str) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::MoveWindow {
            source: source.to_string(),
            target: target.to_string(),
        })?;
        Ok(())
    }

    pub fn renumber_window(
        &self,
        session_name: &str,
        from_window_id: usize,
        to_window_id: usize,
    ) -> Result<(), SubprocessError> {
        self.move_window(
            &format!("{session_name}:{from_window_id}"),
            &format!("{session_name}:{to_window_id}"),
        )
    }

    pub fn rename_window(
        &self,
        session_name: &str,
        window_id: usize,
        name: &str,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::RenameWindow {
            session_name: session_name.to_string(),
            window_id,
            name: name.to_string(),
        })?;
        Ok(())
    }

    pub fn select_window(
        &self,
        session_name: &str,
        window_id: usize,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::SelectWindow {
            session_name: session_name.to_string(),
            window_id,
        })?;
        Ok(())
    }

    pub fn split_window(
        &self,
        session_name: &str,
        window_id: usize,
        pane_min_id: usize,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::SplitWindow {
            session_name: session_name.to_string(),
            window_id,
            pane_min_id,
        })?;
        Ok(())
    }

    pub fn select_layout(
        &self,
        session_name: &str,
        window_id: usize,
        layout: &str,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::SelectLayout {
            session_name: session_name.to_string(),
            window_id,
            layout: layout.to_string(),
        })?;
        Ok(())
    }

    pub fn restore_pane_content(
        &self,
        pane_id: &str,
        filename: &Path,
    ) -> Result<(), SubprocessError> {
        self.run(TmuxCommand::LoadContent {
            pane_id: pane_id.to_string(),
            filename: filename.to_string_lossy().into_owned(),
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxCommand {
    ListSessions,
    ListPanes {
        session_name: String,
        window_index: usize,
    },
    CreateSession {
        session_name: String,
        width: u32,
        height: u32,
    },
    KillSession {
        session_name: String,
    },
    CapturePane {
        pane_id: String,
        include_escape: bool,
    },
    ShowOption {
        option: String,
    },
    HasSession {
        session_name: String,
    },
    SendKeys {
        target: String,
        keys: String,
    },
    ClearPane {
        pane_id: String,
    },
    ListWindows {
        session_name: String,
    },
    MoveWindow {
        source: String,
        target: String,
    },
    RenameWindow {
        session_name: String,
        window_id: usize,
        name: String,
    },
    NewEmptyWindow {
        session_name: String,
        base_index: usize,
    },
    SelectWindow {
        session_name: String,
        window_id: usize,
    },
    SplitWindow {
        session_name: String,
        window_id: usize,
        pane_min_id: usize,
    },
    SelectLayout {
        session_name: String,
        window_id: usize,
        layout: String,
    },
    LoadContent {
        pane_id: String,
        filename: String,
    },
}

impl TmuxCommand {
    fn into_parts(self) -> Vec<String> {
        match self {
            Self::ListSessions => vec![
                TMUX_BIN.to_string(),
                "list-sessions".to_string(),
                format!("-F{LIST_SESSIONS_FORMAT}"),
            ],
            Self::ListPanes {
                session_name,
                window_index,
            } => vec![
                TMUX_BIN.to_string(),
                "list-panes".to_string(),
                format!("-t{session_name}:{window_index}"),
                format!("-F{LIST_PANES_FORMAT}"),
            ],
            Self::CreateSession {
                session_name,
                width,
                height,
            } => vec![
                TMUX_BIN.to_string(),
                "new-session".to_string(),
                "-d".to_string(),
                format!("-s{session_name}"),
                format!("-x{width}"),
                format!("-y{height}"),
            ],
            Self::KillSession { session_name } => vec![
                TMUX_BIN.to_string(),
                "kill-session".to_string(),
                format!("-t{session_name}"),
            ],
            Self::CapturePane {
                pane_id,
                include_escape,
            } => vec![
                TMUX_BIN.to_string(),
                "capture-pane".to_string(),
                format!("-{}p", if include_escape { "e" } else { "" }),
                CAPTURE_HISTORY_START.to_string(),
                format!("-t{pane_id}"),
            ],
            Self::ShowOption { option } => vec![
                TMUX_BIN.to_string(),
                "show-options".to_string(),
                "-gv".to_string(),
                option,
            ],
            Self::HasSession { session_name } => vec![
                TMUX_BIN.to_string(),
                "has-session".to_string(),
                format!("-t{session_name}"),
            ],
            Self::SendKeys { target, keys } => vec![
                TMUX_BIN.to_string(),
                "send-keys".to_string(),
                format!("-t{target}"),
                keys,
            ],
            Self::ClearPane { pane_id } => vec![
                TMUX_BIN.to_string(),
                "clear-history".to_string(),
                format!("-t{pane_id}"),
            ],
            Self::ListWindows { session_name } => vec![
                TMUX_BIN.to_string(),
                "list-windows".to_string(),
                format!("-F{LIST_WINDOWS_FORMAT}"),
                format!("-t{session_name}"),
            ],
            Self::MoveWindow { source, target } => vec![
                TMUX_BIN.to_string(),
                "move-window".to_string(),
                format!("-s{source}"),
                format!("-t{target}"),
            ],
            Self::RenameWindow {
                session_name,
                window_id,
                name,
            } => vec![
                TMUX_BIN.to_string(),
                "rename-window".to_string(),
                format!("-t{session_name}:{window_id}"),
                name,
            ],
            Self::NewEmptyWindow {
                session_name,
                base_index,
            } => vec![
                TMUX_BIN.to_string(),
                "new-window".to_string(),
                "-d".to_string(),
                format!("-t{session_name}:{base_index}"),
            ],
            Self::SelectWindow {
                session_name,
                window_id,
            } => vec![
                TMUX_BIN.to_string(),
                "select-window".to_string(),
                format!("-t{session_name}:{window_id}"),
            ],
            Self::SplitWindow {
                session_name,
                window_id,
                pane_min_id,
            } => vec![
                TMUX_BIN.to_string(),
                "split-window".to_string(),
                "-d".to_string(),
                SPLIT_WINDOW_SIZE.to_string(),
                format!("-t{session_name}:{window_id}.{pane_min_id}"),
            ],
            Self::SelectLayout {
                session_name,
                window_id,
                layout,
            } => vec![
                TMUX_BIN.to_string(),
                "select-layout".to_string(),
                format!("-t{session_name}:{window_id}"),
                layout,
            ],
            Self::LoadContent { pane_id, filename } => vec![
                TMUX_BIN.to_string(),
                "send-keys".to_string(),
                format!("-t{pane_id}"),
                format!("cat   \"{filename}\"\n"),
            ],
        }
    }
}

fn execute_command(
    command: Vec<String>,
    timeout: Option<Duration>,
) -> Result<CommandOutput, SubprocessError> {
    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| match source.kind() {
            std::io::ErrorKind::NotFound => SubprocessError::BinaryNotFound {
                command: command.clone(),
                source,
            },
            _ => SubprocessError::SpawnFailed {
                command: command.clone(),
                source,
            },
        })?;

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
                            stdout: normalize_stream(output.stdout),
                            stderr: normalize_stream(output.stderr),
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

    let output = child
        .wait_with_output()
        .map_err(|source| SubprocessError::WaitFailed {
            command: command.clone(),
            source,
        })?;

    Ok(CommandOutput {
        command,
        status: output.status.code(),
        stdout: normalize_stream(output.stdout),
        stderr: normalize_stream(output.stderr),
    })
}

fn normalize_stream(bytes: Vec<u8>) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

fn split_legacy_lines(output: &str) -> Vec<String> {
    output.split('\n').map(ToOwned::to_owned).collect()
}
