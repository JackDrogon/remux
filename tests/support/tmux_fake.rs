#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::Path;

use remux::error::SubprocessError;
use remux::tmux::{TMUX_BINARY, TmuxAdapter, TmuxClient, TmuxCommand};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeTmuxOutput {
    Unit,
    Bool(bool),
    Lines(Vec<String>),
    Text(String),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub struct FakeTmuxStep {
    command: TmuxCommand,
    result: Result<FakeTmuxOutput, SubprocessError>,
}

impl FakeTmuxStep {
    pub fn ok(command: TmuxCommand, output: FakeTmuxOutput) -> Self {
        Self {
            command,
            result: Ok(output),
        }
    }

    pub fn err(command: TmuxCommand, error: SubprocessError) -> Self {
        Self {
            command,
            result: Err(error),
        }
    }
}

#[derive(Debug)]
pub struct FakeTmux {
    render_adapter: TmuxAdapter,
    include_escape: bool,
    steps: RefCell<VecDeque<FakeTmuxStep>>,
    recorded_commands: RefCell<Vec<TmuxCommand>>,
}

impl FakeTmux {
    pub fn new(steps: impl IntoIterator<Item = FakeTmuxStep>) -> Self {
        Self::from_prefix(vec![TMUX_BINARY.to_string()], true, steps)
    }

    pub fn from_prefix(
        command_prefix: Vec<String>,
        include_escape: bool,
        steps: impl IntoIterator<Item = FakeTmuxStep>,
    ) -> Self {
        Self {
            render_adapter: TmuxAdapter::from_prefix(command_prefix, include_escape),
            include_escape,
            steps: RefCell::new(steps.into_iter().collect()),
            recorded_commands: RefCell::new(Vec::new()),
        }
    }

    pub fn recorded_commands(&self) -> Vec<TmuxCommand> {
        self.recorded_commands.borrow().clone()
    }

    pub fn rendered_commands(&self) -> Vec<Vec<String>> {
        self.recorded_commands()
            .into_iter()
            .map(|command| self.render_adapter.render_command(command))
            .collect()
    }

    pub fn remaining_steps(&self) -> usize {
        self.steps.borrow().len()
    }

    fn consume(&self, command: TmuxCommand) -> Result<FakeTmuxOutput, SubprocessError> {
        self.recorded_commands.borrow_mut().push(command.clone());
        let mut steps = self.steps.borrow_mut();
        let Some(step) = steps.pop_front() else {
            panic!(
                "unexpected fake tmux command: {:?}",
                self.render_adapter.render_command(command)
            );
        };

        assert_eq!(
            step.command,
            command,
            "fake tmux command mismatch: expected {:?}, got {:?}",
            self.render_adapter.render_command(step.command.clone()),
            self.render_adapter.render_command(command.clone())
        );

        step.result
    }

    fn consume_unit(&self, command: TmuxCommand) -> Result<(), SubprocessError> {
        match self.consume(command)? {
            FakeTmuxOutput::Unit => Ok(()),
            other => panic!("expected unit fake tmux output, got {other:?}"),
        }
    }

    fn consume_bool(&self, command: TmuxCommand) -> Result<bool, SubprocessError> {
        match self.consume(command)? {
            FakeTmuxOutput::Bool(value) => Ok(value),
            other => panic!("expected bool fake tmux output, got {other:?}"),
        }
    }

    fn consume_lines(&self, command: TmuxCommand) -> Result<Vec<String>, SubprocessError> {
        match self.consume(command)? {
            FakeTmuxOutput::Lines(lines) => Ok(lines),
            other => panic!("expected line fake tmux output, got {other:?}"),
        }
    }

    fn consume_text(&self, command: TmuxCommand) -> Result<String, SubprocessError> {
        match self.consume(command)? {
            FakeTmuxOutput::Text(text) => Ok(text),
            other => panic!("expected text fake tmux output, got {other:?}"),
        }
    }

    fn consume_bytes(&self, command: TmuxCommand) -> Result<Vec<u8>, SubprocessError> {
        match self.consume(command)? {
            FakeTmuxOutput::Bytes(bytes) => Ok(bytes),
            other => panic!("expected byte fake tmux output, got {other:?}"),
        }
    }
}

impl TmuxClient for FakeTmux {
    fn has_server(&self) -> Result<bool, SubprocessError> {
        self.consume_bool(TmuxCommand::ListSessions)
    }

    fn list_sessions(&self) -> Result<Vec<String>, SubprocessError> {
        self.consume_lines(TmuxCommand::ListSessions)
    }

    fn list_windows(&self, session_name: &str) -> Result<Vec<String>, SubprocessError> {
        self.consume_lines(TmuxCommand::ListWindows {
            session_name: session_name.to_string(),
        })
    }

    fn list_panes(
        &self,
        session_name: &str,
        window_index: usize,
    ) -> Result<Vec<String>, SubprocessError> {
        self.consume_lines(TmuxCommand::ListPanes {
            session_name: session_name.to_string(),
            window_index,
        })
    }

    fn create_session(
        &self,
        session_name: &str,
        width: u32,
        height: u32,
    ) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::CreateSession {
            session_name: session_name.to_string(),
            width,
            height,
        })
    }

    fn kill_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        self.consume_bool(TmuxCommand::KillSession {
            session_name: session_name.to_string(),
        })
    }

    fn capture_pane(&self, pane_id: &str) -> Result<String, SubprocessError> {
        self.consume_text(TmuxCommand::CapturePane {
            pane_id: pane_id.to_string(),
            include_escape: self.include_escape,
        })
    }

    fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>, SubprocessError> {
        self.consume_bytes(TmuxCommand::CapturePane {
            pane_id: pane_id.to_string(),
            include_escape: self.include_escape,
        })
    }

    fn show_option(&self, option: &str) -> Result<String, SubprocessError> {
        self.consume_text(TmuxCommand::ShowOption {
            option: option.to_string(),
        })
    }

    fn has_session(&self, session_name: &str) -> Result<bool, SubprocessError> {
        self.consume_bool(TmuxCommand::HasSession {
            session_name: session_name.to_string(),
        })
    }

    fn clear_pane(&self, pane_id: &str) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::ClearPane {
            pane_id: pane_id.to_string(),
        })
    }

    fn send_keys(&self, target: &str, keys: &str) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::SendKeys {
            target: target.to_string(),
            keys: keys.to_string(),
        })
    }

    fn create_empty_window(
        &self,
        session_name: &str,
        base_index: usize,
    ) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::NewEmptyWindow {
            session_name: session_name.to_string(),
            base_index,
        })
    }

    fn move_window(&self, source: &str, target: &str) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::MoveWindow {
            source: source.to_string(),
            target: target.to_string(),
        })
    }

    fn rename_window(
        &self,
        session_name: &str,
        window_id: usize,
        name: &str,
    ) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::RenameWindow {
            session_name: session_name.to_string(),
            window_id,
            name: name.to_string(),
        })
    }

    fn select_window(&self, session_name: &str, window_id: usize) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::SelectWindow {
            session_name: session_name.to_string(),
            window_id,
        })
    }

    fn split_window(
        &self,
        session_name: &str,
        window_id: usize,
        pane_min_id: usize,
    ) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::SplitWindow {
            session_name: session_name.to_string(),
            window_id,
            pane_min_id,
        })
    }

    fn select_layout(
        &self,
        session_name: &str,
        window_id: usize,
        layout: &str,
    ) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::SelectLayout {
            session_name: session_name.to_string(),
            window_id,
            layout: layout.to_string(),
        })
    }

    fn restore_pane_content(&self, pane_id: &str, filename: &Path) -> Result<(), SubprocessError> {
        self.consume_unit(TmuxCommand::LoadContent {
            pane_id: pane_id.to_string(),
            filename: filename.to_string_lossy().into_owned(),
        })
    }
}
