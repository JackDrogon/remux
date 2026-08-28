use std::path::Path;

use super::TMUX_BINARY;
use super::client::TmuxClient;
use super::command::TmuxCommand;
use super::subprocess::{
    CommandOutput, SubprocessExecutor, SubprocessRunner, normalize_output_stream,
};
use crate::{Result, Tmux as TmuxError};

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
            subprocess: SubprocessRunner,
        }
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

    pub fn execute_without_status_check(&self, command: TmuxCommand) -> Result<CommandOutput> {
        let command = self.render_command(command);
        self.subprocess.execute(command)
    }

    pub fn run(&self, command: TmuxCommand) -> Result<CommandOutput> {
        let output = self.execute_without_status_check(command)?;
        if output.success() {
            Ok(output)
        } else {
            Err(TmuxError::TmuxFailed {
                command: output.command,
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            }
            .into())
        }
    }

    pub fn has_server(&self) -> Result<bool> {
        self.probe(TmuxCommand::ListSessions, PresenceProbe::Server)
    }

    pub fn list_sessions(&self) -> Result<Vec<String>> {
        self.execute_listing_command(TmuxCommand::ListSessions)
    }

    pub fn list_windows(&self, session_name: &str) -> Result<Vec<String>> {
        self.execute_listing_command(TmuxCommand::ListWindows {
            session_name: session_name.to_string(),
        })
    }

    pub fn list_panes(&self, session_name: &str, window_index: usize) -> Result<Vec<String>> {
        self.execute_listing_command(TmuxCommand::ListPanes {
            session_name: session_name.to_string(),
            window_index,
        })
    }

    pub fn create_session(&self, session_name: &str, width: u32, height: u32) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::CreateSession {
            session_name: session_name.to_string(),
            width,
            height,
        })
    }

    pub fn kill_session(&self, session_name: &str) -> Result<bool> {
        self.probe(
            TmuxCommand::KillSession {
                session_name: session_name.to_string(),
            },
            PresenceProbe::Session,
        )
    }

    pub fn capture_pane(&self, pane_id: &str) -> Result<String> {
        self.execute_text_command(self.capture_pane_command(pane_id))
    }

    pub fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>> {
        self.execute_byte_command(self.capture_pane_command(pane_id))
    }

    pub fn show_option(&self, option: &str) -> Result<String> {
        self.execute_text_command(TmuxCommand::ShowOption {
            option: option.to_string(),
        })
    }

    pub fn has_session(&self, session_name: &str) -> Result<bool> {
        self.probe(
            TmuxCommand::HasSession {
                session_name: session_name.to_string(),
            },
            PresenceProbe::Session,
        )
    }

    pub fn clear_pane(&self, pane_id: &str) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::ClearPane {
            pane_id: pane_id.to_string(),
        })
    }

    pub fn send_keys(&self, target: &str, keys: &str) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::SendKeys {
            target: target.to_string(),
            keys: keys.to_string(),
        })
    }

    pub fn set_pane_path(&self, pane_id: &str, path: &Path) -> Result<()> {
        TmuxClient::set_pane_path(self, pane_id, path)
    }

    pub fn create_empty_window(&self, session_name: &str, base_index: usize) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::NewEmptyWindow {
            session_name: session_name.to_string(),
            base_index,
        })
    }

    pub fn move_window(&self, source: &str, target: &str) -> Result<()> {
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
    ) -> Result<()> {
        TmuxClient::renumber_window(self, session_name, from_window_id, to_window_id)
    }

    pub fn rename_window(&self, session_name: &str, window_id: usize, name: &str) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::RenameWindow {
            session_name: session_name.to_string(),
            window_id,
            name: name.to_string(),
        })
    }

    pub fn select_window(&self, session_name: &str, window_id: usize) -> Result<()> {
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
    ) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::SplitWindow {
            session_name: session_name.to_string(),
            window_id,
            pane_min_id,
        })
    }

    pub fn select_layout(&self, session_name: &str, window_id: usize, layout: &str) -> Result<()> {
        self.execute_mutating_command(TmuxCommand::SelectLayout {
            session_name: session_name.to_string(),
            window_id,
            layout: layout.to_string(),
        })
    }

    pub fn restore_pane_content(&self, pane_id: &str, filename: &Path) -> Result<()> {
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

    /// tmux boolean commands: 0 = present, 1 = absent. Spawn/wait failures
    /// stay `Err`. Absence is decided on the raw completed output; a constructed
    /// `TmuxFailed` is never rewritten to `Ok(false)`.
    fn probe(&self, command: TmuxCommand, presence: PresenceProbe) -> Result<bool> {
        let output = self.execute_without_status_check(command)?;
        match classify_probe(&output, presence) {
            ProbeOutcome::Present => Ok(true),
            ProbeOutcome::Absent => Ok(false),
            ProbeOutcome::Failed => Err(TmuxError::TmuxFailed {
                command: output.command,
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            }
            .into()),
        }
    }

    fn execute_listing_command(&self, command: TmuxCommand) -> Result<Vec<String>> {
        Ok(split_tmux_lines(&self.run(command)?.stdout))
    }

    fn execute_text_command(&self, command: TmuxCommand) -> Result<String> {
        Ok(self.run(command)?.stdout)
    }

    fn execute_mutating_command(&self, command: TmuxCommand) -> Result<()> {
        self.run(command)?;
        Ok(())
    }

    fn execute_byte_command(&self, command: TmuxCommand) -> Result<Vec<u8>> {
        let output = self
            .subprocess
            .execute_bytes(self.render_command(command))?;

        if output.success() {
            return Ok(output.stdout);
        }

        Err(TmuxError::TmuxFailed {
            command: output.command,
            status: output.status,
            stdout: normalize_output_stream(output.stdout),
            stderr: normalize_output_stream(output.stderr),
        }
        .into())
    }
}

impl<E> TmuxClient for TmuxAdapter<E>
where
    E: SubprocessExecutor,
{
    fn has_server(&self) -> Result<bool> {
        TmuxAdapter::has_server(self)
    }

    fn list_sessions(&self) -> Result<Vec<String>> {
        TmuxAdapter::list_sessions(self)
    }

    fn list_windows(&self, session_name: &str) -> Result<Vec<String>> {
        TmuxAdapter::list_windows(self, session_name)
    }

    fn list_panes(&self, session_name: &str, window_index: usize) -> Result<Vec<String>> {
        TmuxAdapter::list_panes(self, session_name, window_index)
    }

    fn create_session(&self, session_name: &str, width: u32, height: u32) -> Result<()> {
        TmuxAdapter::create_session(self, session_name, width, height)
    }

    fn kill_session(&self, session_name: &str) -> Result<bool> {
        TmuxAdapter::kill_session(self, session_name)
    }

    fn capture_pane(&self, pane_id: &str) -> Result<String> {
        TmuxAdapter::capture_pane(self, pane_id)
    }

    fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>> {
        TmuxAdapter::capture_pane_bytes(self, pane_id)
    }

    fn show_option(&self, option: &str) -> Result<String> {
        TmuxAdapter::show_option(self, option)
    }

    fn has_session(&self, session_name: &str) -> Result<bool> {
        TmuxAdapter::has_session(self, session_name)
    }

    fn clear_pane(&self, pane_id: &str) -> Result<()> {
        TmuxAdapter::clear_pane(self, pane_id)
    }

    fn send_keys(&self, target: &str, keys: &str) -> Result<()> {
        TmuxAdapter::send_keys(self, target, keys)
    }

    fn create_empty_window(&self, session_name: &str, base_index: usize) -> Result<()> {
        TmuxAdapter::create_empty_window(self, session_name, base_index)
    }

    fn move_window(&self, source: &str, target: &str) -> Result<()> {
        TmuxAdapter::move_window(self, source, target)
    }

    fn rename_window(&self, session_name: &str, window_id: usize, name: &str) -> Result<()> {
        TmuxAdapter::rename_window(self, session_name, window_id, name)
    }

    fn select_window(&self, session_name: &str, window_id: usize) -> Result<()> {
        TmuxAdapter::select_window(self, session_name, window_id)
    }

    fn split_window(&self, session_name: &str, window_id: usize, pane_min_id: usize) -> Result<()> {
        TmuxAdapter::split_window(self, session_name, window_id, pane_min_id)
    }

    fn select_layout(&self, session_name: &str, window_id: usize, layout: &str) -> Result<()> {
        TmuxAdapter::select_layout(self, session_name, window_id, layout)
    }

    fn restore_pane_content(&self, pane_id: &str, filename: &Path) -> Result<()> {
        TmuxAdapter::restore_pane_content(self, pane_id, filename)
    }
}

fn split_tmux_lines(output: &str) -> Vec<String> {
    output.split('\n').map(ToOwned::to_owned).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresenceProbe {
    /// `list-sessions` as a server check: empty stderr is hard failure.
    Server,
    /// `has-session` / `kill-session`: empty stderr is absence.
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeOutcome {
    Present,
    Absent,
    Failed,
}

fn classify_probe(output: &CommandOutput, presence: PresenceProbe) -> ProbeOutcome {
    if output.success() {
        return ProbeOutcome::Present;
    }
    if output.status == Some(1) && tmux_reports_absence(&output.stderr, presence) {
        return ProbeOutcome::Absent;
    }
    ProbeOutcome::Failed
}

fn tmux_reports_absence(stderr: &str, presence: PresenceProbe) -> bool {
    let text = stderr.trim();
    let lower = text.to_ascii_lowercase();
    match presence {
        PresenceProbe::Server => {
            !text.is_empty()
                && (lower.contains("no server running")
                    || lower.contains("no such file or directory"))
        }
        PresenceProbe::Session => {
            text.is_empty()
                || lower.contains("no server running")
                || lower.contains("no such file or directory")
                || lower.contains("can't find session")
                || lower.contains("cannot find session")
                || lower.contains("session not found")
                || lower.contains("unknown session")
                || lower.contains("no current")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use super::*;
    #[derive(Debug)]
    struct FakeExecutorStep {
        command: Vec<String>,
        output: Result<CommandOutput>,
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
        fn execute(&self, command: Vec<String>) -> Result<CommandOutput> {
            let Some(step) = self.steps.borrow_mut().pop_front() else {
                panic!("unexpected fake executor command: {command:?}");
            };
            assert_eq!(step.command, command);
            step.output
        }

        fn execute_bytes(
            &self,
            command: Vec<String>,
        ) -> Result<super::super::subprocess::ByteCommandOutput> {
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

    fn list_sessions_command() -> Vec<String> {
        vec![
            "tmux".to_string(),
            "list-sessions".to_string(),
            "-F#S:=:(#{window_width},#{window_height}):=:#{session_attached}".to_string(),
        ]
    }

    fn list_sessions_output(status: Option<i32>, stderr: &str) -> CommandOutput {
        CommandOutput {
            command: list_sessions_command(),
            status,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn probe_allowlist_is_table_driven() {
        let cases = [
            (Some(0), "", PresenceProbe::Server, ProbeOutcome::Present),
            (Some(1), "", PresenceProbe::Server, ProbeOutcome::Failed),
            (Some(1), "", PresenceProbe::Session, ProbeOutcome::Absent),
            (
                Some(1),
                "  NO SERVER RUNNING on /tmp/x  ",
                PresenceProbe::Server,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "No Such File Or Directory",
                PresenceProbe::Server,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "can't find session: demo",
                PresenceProbe::Session,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "cannot find session: demo",
                PresenceProbe::Session,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "session not found",
                PresenceProbe::Session,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "unknown session",
                PresenceProbe::Session,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "no current",
                PresenceProbe::Session,
                ProbeOutcome::Absent,
            ),
            (
                Some(1),
                "no current",
                PresenceProbe::Server,
                ProbeOutcome::Failed,
            ),
            (
                Some(1),
                "can't find session: demo",
                PresenceProbe::Server,
                ProbeOutcome::Failed,
            ),
            (
                Some(1),
                "Permission denied",
                PresenceProbe::Server,
                ProbeOutcome::Failed,
            ),
            (Some(2), "", PresenceProbe::Session, ProbeOutcome::Failed),
            (None, "", PresenceProbe::Session, ProbeOutcome::Failed),
        ];
        for (status, stderr, probe, expected) in cases {
            assert_eq!(
                classify_probe(&list_sessions_output(status, stderr), probe),
                expected,
                "status={status:?} stderr={stderr:?} probe={probe:?}"
            );
        }
    }

    #[test]
    fn probe_treats_tmux_absence_as_false() {
        assert_eq!(
            classify_probe(&list_sessions_output(Some(0), ""), PresenceProbe::Server),
            ProbeOutcome::Present
        );
        assert_eq!(
            classify_probe(&list_sessions_output(Some(1), ""), PresenceProbe::Session),
            ProbeOutcome::Absent
        );
        assert_eq!(
            classify_probe(&list_sessions_output(Some(1), ""), PresenceProbe::Server),
            ProbeOutcome::Failed
        );
        assert_eq!(
            classify_probe(
                &list_sessions_output(Some(1), "no server running on /tmp/tmux-1000/default",),
                PresenceProbe::Server
            ),
            ProbeOutcome::Absent
        );
        assert_eq!(
            classify_probe(
                &list_sessions_output(
                    Some(1),
                    "error connecting to /tmp/tmux-1000/default (No such file or directory)",
                ),
                PresenceProbe::Server
            ),
            ProbeOutcome::Absent
        );
        assert_eq!(
            classify_probe(
                &list_sessions_output(Some(1), "can't find session: demo"),
                PresenceProbe::Session
            ),
            ProbeOutcome::Absent
        );
    }

    #[test]
    fn probe_does_not_treat_hard_failures_as_absence() {
        assert_eq!(
            classify_probe(
                &list_sessions_output(
                    Some(1),
                    "error connecting to /tmp/tmux-1000/default (Permission denied)",
                ),
                PresenceProbe::Server
            ),
            ProbeOutcome::Failed
        );
        assert_eq!(
            classify_probe(&list_sessions_output(None, ""), PresenceProbe::Session),
            ProbeOutcome::Failed
        );
        assert_eq!(
            classify_probe(&list_sessions_output(Some(2), ""), PresenceProbe::Session),
            ProbeOutcome::Failed
        );
        assert_eq!(
            classify_probe(
                &list_sessions_output(Some(1), "can't find session: demo"),
                PresenceProbe::Server
            ),
            ProbeOutcome::Failed
        );
    }

    #[test]
    fn has_server_returns_tmux_failed_for_permission_denied() {
        let command = list_sessions_command();
        let fake = FakeExecutor::new([FakeExecutorStep {
            command: command.clone(),
            output: Ok(list_sessions_output(
                Some(1),
                "error connecting to /tmp/tmux-1000/default (Permission denied)",
            )),
        }]);
        let adapter =
            TmuxAdapter::from_prefix(vec!["tmux".to_string()], true).with_subprocess(fake);

        let error = adapter
            .has_server()
            .expect_err("permission denied must not look like no server");
        assert!(matches!(
            error.code(),
            crate::Code::Tmux(TmuxError::TmuxFailed {
                status: Some(1),
                ..
            })
        ));
    }

    #[test]
    fn has_server_returns_false_when_tmux_reports_no_server() {
        let fake = FakeExecutor::new([FakeExecutorStep {
            command: list_sessions_command(),
            output: Ok(list_sessions_output(
                Some(1),
                "no server running on /tmp/tmux-1000/default",
            )),
        }]);
        let adapter =
            TmuxAdapter::from_prefix(vec!["tmux".to_string()], true).with_subprocess(fake);

        assert!(
            !adapter
                .has_server()
                .expect("no server is a successful absence probe")
        );
    }

    #[test]
    fn has_server_silent_nonzero_exit_is_failure() {
        let fake = FakeExecutor::new([FakeExecutorStep {
            command: list_sessions_command(),
            output: Ok(list_sessions_output(Some(1), "")),
        }]);
        let adapter =
            TmuxAdapter::from_prefix(vec!["tmux".to_string()], true).with_subprocess(fake);

        let error = adapter
            .has_server()
            .expect_err("silent list-sessions failure must not look like no server");
        assert!(matches!(
            error.code(),
            crate::Code::Tmux(TmuxError::TmuxFailed {
                status: Some(1),
                ..
            })
        ));
    }
}
