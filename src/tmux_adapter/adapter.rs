use std::path::Path;
use std::time::Duration;

use super::TMUX_BINARY;
use super::client::TmuxClient;
use super::command::TmuxCommand;
use super::error::SubprocessError;
use super::subprocess::{
    CommandOutput, SubprocessExecutor, SubprocessRunner, normalize_output_stream,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxAdapter<E = SubprocessRunner> {
    command_prefix: Vec<String>,
    content_with_escape: bool,
    subprocess: E,
}

impl TmuxAdapter<SubprocessRunner> {
    pub fn from_prefix(command_prefix: Vec<String>, content_with_escape: bool) -> Self {
        Self {
            command_prefix,
            content_with_escape,
            subprocess: SubprocessRunner::default(),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.subprocess = self.subprocess.with_timeout(timeout);
        self
    }
}

impl<E> TmuxAdapter<E> {
    pub fn with_subprocess<E2>(self, subprocess: E2) -> TmuxAdapter<E2> {
        TmuxAdapter {
            command_prefix: self.command_prefix,
            content_with_escape: self.content_with_escape,
            subprocess,
        }
    }
}

impl<E> TmuxAdapter<E>
where
    E: SubprocessExecutor,
{
    pub fn command_prefix(&self) -> &[String] {
        &self.command_prefix
    }

    pub fn render_command(&self, command: TmuxCommand) -> Vec<String> {
        let parts = command.into_parts();
        if parts.first().map(|part| part.as_str()) == Some(TMUX_BINARY) {
            let mut rendered = self.command_prefix.clone();
            rendered.extend(parts.into_iter().skip(1));
            rendered
        } else {
            parts
        }
    }

    pub fn execute_without_status_check(
        &self,
        command: TmuxCommand,
    ) -> Result<CommandOutput, SubprocessError> {
        let command = self.render_command(command);
        self.subprocess.execute(command)
    }

    pub fn run(&self, command: TmuxCommand) -> Result<CommandOutput, SubprocessError> {
        let output = self.execute_without_status_check(command)?;
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
        self.command_succeeds(TmuxCommand::ListSessions)
    }

    pub fn list_sessions(&self) -> Result<Vec<String>, SubprocessError> {
        self.execute_listing_command(TmuxCommand::ListSessions)
    }

    pub fn list_windows(&self, session_name: &str) -> Result<Vec<String>, SubprocessError> {
        self.execute_listing_command(TmuxCommand::ListWindows {
            session_name: session_name.to_string(),
        })
    }

    pub fn list_panes(
        &self,
        session_name: &str,
        window_index: usize,
    ) -> Result<Vec<String>, SubprocessError> {
        self.execute_listing_command(TmuxCommand::ListPanes {
            session_name: session_name.to_string(),
            window_index,
        })
    }

    pub fn create_session(
        &self,
        session_name: &str,
        width: u32,
        height: u32,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::CreateSession {
            session_name: session_name.to_string(),
            width,
            height,
        })
    }

    pub fn kill_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        self.command_succeeds(TmuxCommand::KillSession {
            session_name: session_name.to_string(),
        })
    }

    pub fn capture_pane(&self, pane_id: &str) -> Result<String, SubprocessError> {
        self.execute_text_command(self.capture_pane_command(pane_id))
    }

    pub fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>, SubprocessError> {
        self.execute_byte_command(self.capture_pane_command(pane_id))
    }

    pub fn show_option(&self, option: &str) -> Result<String, SubprocessError> {
        self.execute_text_command(TmuxCommand::ShowOption {
            option: option.to_string(),
        })
    }

    pub fn has_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        self.command_succeeds(TmuxCommand::HasSession {
            session_name: session_name.to_string(),
        })
    }

    pub fn clear_pane(&self, pane_id: &str) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::ClearPane {
            pane_id: pane_id.to_string(),
        })
    }

    pub fn send_keys(&self, target: &str, keys: &str) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::SendKeys {
            target: target.to_string(),
            keys: keys.to_string(),
        })
    }

    pub fn set_pane_path(&self, pane_id: &str, path: &Path) -> Result<(), SubprocessError> {
        TmuxClient::set_pane_path(self, pane_id, path)
    }

    pub fn create_empty_window(
        &self,
        session_name: &str,
        base_index: usize,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::NewEmptyWindow {
            session_name: session_name.to_string(),
            base_index,
        })
    }

    pub fn move_window(&self, source: &str, target: &str) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::MoveWindow {
            source: source.to_string(),
            target: target.to_string(),
        })
    }

    pub fn renumber_window(
        &self,
        session_name: &str,
        from_window_id: usize,
        to_window_id: usize,
    ) -> Result<(), SubprocessError> {
        TmuxClient::renumber_window(self, session_name, from_window_id, to_window_id)
    }

    pub fn rename_window(
        &self,
        session_name: &str,
        window_id: usize,
        name: &str,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::RenameWindow {
            session_name: session_name.to_string(),
            window_id,
            name: name.to_string(),
        })
    }

    pub fn select_window(
        &self,
        session_name: &str,
        window_id: usize,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::SelectWindow {
            session_name: session_name.to_string(),
            window_id,
        })
    }

    pub fn split_window(
        &self,
        session_name: &str,
        window_id: usize,
        pane_min_id: usize,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::SplitWindow {
            session_name: session_name.to_string(),
            window_id,
            pane_min_id,
        })
    }

    pub fn select_layout(
        &self,
        session_name: &str,
        window_id: usize,
        layout: &str,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::SelectLayout {
            session_name: session_name.to_string(),
            window_id,
            layout: layout.to_string(),
        })
    }

    pub fn restore_pane_content(
        &self,
        pane_id: &str,
        filename: &Path,
    ) -> Result<(), SubprocessError> {
        self.execute_mutating_command(TmuxCommand::LoadContent {
            pane_id: pane_id.to_string(),
            filename: filename.to_string_lossy().into_owned(),
        })
    }

    fn capture_pane_command(&self, pane_id: &str) -> TmuxCommand {
        TmuxCommand::CapturePane {
            pane_id: pane_id.to_string(),
            include_escape: self.content_with_escape,
        }
    }

    fn command_succeeds(&self, command: TmuxCommand) -> Result<bool, SubprocessError> {
        Ok(self.execute_without_status_check(command)?.success())
    }

    fn execute_listing_command(
        &self,
        command: TmuxCommand,
    ) -> Result<Vec<String>, SubprocessError> {
        Ok(split_tmux_lines(&self.run(command)?.stdout))
    }

    fn execute_text_command(&self, command: TmuxCommand) -> Result<String, SubprocessError> {
        Ok(self.run(command)?.stdout)
    }

    fn execute_mutating_command(&self, command: TmuxCommand) -> Result<(), SubprocessError> {
        self.run(command)?;
        Ok(())
    }

    fn execute_byte_command(&self, command: TmuxCommand) -> Result<Vec<u8>, SubprocessError> {
        let output = self
            .subprocess
            .execute_bytes(self.render_command(command))?;

        if output.success() {
            return Ok(output.stdout);
        }

        Err(SubprocessError::Failed {
            command: output.command,
            status: output.status,
            stdout: normalize_output_stream(output.stdout),
            stderr: normalize_output_stream(output.stderr),
        })
    }
}

impl<E> TmuxClient for TmuxAdapter<E>
where
    E: SubprocessExecutor,
{
    fn has_server(&self) -> Result<bool, SubprocessError> {
        TmuxAdapter::has_server(self)
    }

    fn list_sessions(&self) -> Result<Vec<String>, SubprocessError> {
        TmuxAdapter::list_sessions(self)
    }

    fn list_windows(&self, session_name: &str) -> Result<Vec<String>, SubprocessError> {
        TmuxAdapter::list_windows(self, session_name)
    }

    fn list_panes(
        &self,
        session_name: &str,
        window_index: usize,
    ) -> Result<Vec<String>, SubprocessError> {
        TmuxAdapter::list_panes(self, session_name, window_index)
    }

    fn create_session(
        &self,
        session_name: &str,
        width: u32,
        height: u32,
    ) -> Result<(), SubprocessError> {
        TmuxAdapter::create_session(self, session_name, width, height)
    }

    fn kill_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        TmuxAdapter::kill_session(self, session_name)
    }

    fn capture_pane(&self, pane_id: &str) -> Result<String, SubprocessError> {
        TmuxAdapter::capture_pane(self, pane_id)
    }

    fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>, SubprocessError> {
        TmuxAdapter::capture_pane_bytes(self, pane_id)
    }

    fn show_option(&self, option: &str) -> Result<String, SubprocessError> {
        TmuxAdapter::show_option(self, option)
    }

    fn has_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        TmuxAdapter::has_session(self, session_name)
    }

    fn clear_pane(&self, pane_id: &str) -> Result<(), SubprocessError> {
        TmuxAdapter::clear_pane(self, pane_id)
    }

    fn send_keys(&self, target: &str, keys: &str) -> Result<(), SubprocessError> {
        TmuxAdapter::send_keys(self, target, keys)
    }

    fn create_empty_window(
        &self,
        session_name: &str,
        base_index: usize,
    ) -> Result<(), SubprocessError> {
        TmuxAdapter::create_empty_window(self, session_name, base_index)
    }

    fn move_window(&self, source: &str, target: &str) -> Result<(), SubprocessError> {
        TmuxAdapter::move_window(self, source, target)
    }

    fn rename_window(
        &self,
        session_name: &str,
        window_id: usize,
        name: &str,
    ) -> Result<(), SubprocessError> {
        TmuxAdapter::rename_window(self, session_name, window_id, name)
    }

    fn select_window(&self, session_name: &str, window_id: usize) -> Result<(), SubprocessError> {
        TmuxAdapter::select_window(self, session_name, window_id)
    }

    fn split_window(
        &self,
        session_name: &str,
        window_id: usize,
        pane_min_id: usize,
    ) -> Result<(), SubprocessError> {
        TmuxAdapter::split_window(self, session_name, window_id, pane_min_id)
    }

    fn select_layout(
        &self,
        session_name: &str,
        window_id: usize,
        layout: &str,
    ) -> Result<(), SubprocessError> {
        TmuxAdapter::select_layout(self, session_name, window_id, layout)
    }

    fn restore_pane_content(&self, pane_id: &str, filename: &Path) -> Result<(), SubprocessError> {
        TmuxAdapter::restore_pane_content(self, pane_id, filename)
    }
}

fn split_tmux_lines(output: &str) -> Vec<String> {
    output.split('\n').map(ToOwned::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;
    #[derive(Debug)]
    struct FakeExecutorStep {
        command: Vec<String>,
        output: Result<CommandOutput, SubprocessError>,
    }

    #[derive(Debug, Default)]
    struct FakeExecutor {
        steps: RefCell<VecDeque<FakeExecutorStep>>,
        bytes_calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeExecutor {
        fn new(steps: impl IntoIterator<Item = FakeExecutorStep>) -> Self {
            Self {
                steps: RefCell::new(steps.into_iter().collect()),
                bytes_calls: RefCell::new(Vec::new()),
            }
        }

        fn bytes_calls(&self) -> Vec<Vec<String>> {
            self.bytes_calls.borrow().clone()
        }
    }

    impl SubprocessExecutor for FakeExecutor {
        fn execute(&self, command: Vec<String>) -> Result<CommandOutput, SubprocessError> {
            let Some(step) = self.steps.borrow_mut().pop_front() else {
                panic!("unexpected fake executor command: {command:?}");
            };
            assert_eq!(step.command, command);
            step.output
        }

        fn execute_bytes(
            &self,
            command: Vec<String>,
        ) -> Result<super::super::subprocess::ByteCommandOutput, SubprocessError> {
            self.bytes_calls.borrow_mut().push(command);
            panic!("byte execution is not used in this test")
        }
    }

    #[test]
    fn adapter_can_use_fake_subprocess_executor() {
        let fake = FakeExecutor::new([FakeExecutorStep {
            command: vec![
                "tmux".to_string(),
                "has-session".to_string(),
                "-tdemo".to_string(),
            ],
            output: Ok(CommandOutput {
                command: vec![
                    "tmux".to_string(),
                    "has-session".to_string(),
                    "-tdemo".to_string(),
                ],
                status: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            }),
        }]);
        let adapter =
            TmuxAdapter::from_prefix(vec!["tmux".to_string()], true).with_subprocess(fake);

        assert!(
            adapter
                .has_session("demo")
                .expect("fake subprocess should be used")
        );
        assert!(adapter.subprocess.bytes_calls().is_empty());
    }
}
