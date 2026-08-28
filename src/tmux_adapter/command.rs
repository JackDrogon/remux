use std::path::Path;

use crate::model::{PaneTarget, WindowTarget};

pub const TMUX_BINARY: &str = "tmux";
pub const OUTPUT_SEPARATOR: &str = ":=:";
pub const LIST_SESSIONS_FORMAT: &str =
    "#S:=:(#{window_width},#{window_height}):=:#{session_attached}";
pub const LIST_PANES_FORMAT: &str = "#{pane_index}:=:(#{pane_width},#{pane_height}):=:#{pane_current_path}:=:#{pane_active}:=:#{pane_current_command}:=:#{pane_pid}";
pub const LIST_WINDOWS_FORMAT: &str =
    "#{window_index}:=:#{window_name}:=:#{window_active}:=:#{window_layout}";

const CAPTURE_HISTORY_START: &str = "-S-100000";
const SPLIT_WINDOW_SIZE: &str = "-l3";

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum TmuxCommandArg {
    Flag(String),
    Format(String),
    Target(String),
    Positional(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TmuxCommandBuilder {
    subcommand: &'static str,
    args: Vec<TmuxCommandArg>,
}

impl TmuxCommandBuilder {
    fn new(subcommand: &'static str) -> Self {
        Self {
            subcommand,
            args: Vec::new(),
        }
    }

    fn flag(mut self, flag: impl Into<String>) -> Self {
        self.args.push(TmuxCommandArg::Flag(flag.into()));
        self
    }

    fn format(mut self, format: impl Into<String>) -> Self {
        self.args.push(TmuxCommandArg::Format(format.into()));
        self
    }

    fn target(mut self, target: impl Into<String>) -> Self {
        self.args.push(TmuxCommandArg::Target(target.into()));
        self
    }

    fn positional(mut self, value: impl Into<String>) -> Self {
        self.args.push(TmuxCommandArg::Positional(value.into()));
        self
    }

    fn build(self) -> Vec<String> {
        let mut parts = Vec::with_capacity(self.args.len() + 2);
        parts.push(TMUX_BINARY.to_string());
        parts.push(self.subcommand.to_string());
        parts.extend(self.args.into_iter().map(TmuxCommandArg::render));
        parts
    }
}

impl TmuxCommandArg {
    fn render(self) -> String {
        match self {
            Self::Flag(flag) => format!("-{flag}"),
            Self::Format(format) => format!("-F{format}"),
            Self::Target(target) => format!("-t{target}"),
            Self::Positional(value) => value,
        }
    }
}

impl TmuxCommand {
    pub(crate) fn into_parts(self) -> Vec<String> {
        match self {
            Self::ListSessions => TmuxCommandBuilder::new("list-sessions")
                .format(LIST_SESSIONS_FORMAT)
                .build(),
            Self::ListPanes {
                session_name,
                window_index,
            } => TmuxCommandBuilder::new("list-panes")
                .target(window_target(&session_name, window_index))
                .format(LIST_PANES_FORMAT)
                .build(),
            Self::CreateSession {
                session_name,
                width,
                height,
            } => TmuxCommandBuilder::new("new-session")
                .flag("d")
                .flag(format!("s{session_name}"))
                .flag(format!("x{width}"))
                .flag(format!("y{height}"))
                .build(),
            Self::KillSession { session_name } => TmuxCommandBuilder::new("kill-session")
                .target(session_name)
                .build(),
            Self::CapturePane {
                pane_id,
                include_escape,
            } => TmuxCommandBuilder::new("capture-pane")
                .flag(if include_escape { "ep" } else { "p" })
                .positional(CAPTURE_HISTORY_START)
                .target(pane_id)
                .build(),
            Self::ShowOption { option } => TmuxCommandBuilder::new("show-options")
                .flag("gv")
                .positional(option)
                .build(),
            Self::HasSession { session_name } => TmuxCommandBuilder::new("has-session")
                .target(session_name)
                .build(),
            Self::SendKeys { target, keys } => TmuxCommandBuilder::new("send-keys")
                .target(target)
                .positional(keys)
                .build(),
            Self::ClearPane { pane_id } => TmuxCommandBuilder::new("clear-history")
                .target(pane_id)
                .build(),
            Self::ListWindows { session_name } => TmuxCommandBuilder::new("list-windows")
                .format(LIST_WINDOWS_FORMAT)
                .target(session_name)
                .build(),
            Self::MoveWindow { source, target } => TmuxCommandBuilder::new("move-window")
                .flag(format!("s{source}"))
                .target(target)
                .build(),
            Self::RenameWindow {
                session_name,
                window_id,
                name,
            } => TmuxCommandBuilder::new("rename-window")
                .target(window_target(&session_name, window_id))
                .positional(name)
                .build(),
            Self::NewEmptyWindow {
                session_name,
                base_index,
            } => TmuxCommandBuilder::new("new-window")
                .flag("d")
                .target(window_target(&session_name, base_index))
                .build(),
            Self::SelectWindow {
                session_name,
                window_id,
            } => TmuxCommandBuilder::new("select-window")
                .target(window_target(&session_name, window_id))
                .build(),
            Self::SplitWindow {
                session_name,
                window_id,
                pane_min_id,
            } => TmuxCommandBuilder::new("split-window")
                .flag("d")
                .positional(SPLIT_WINDOW_SIZE)
                .target(pane_target(&session_name, window_id, pane_min_id))
                .build(),
            Self::SelectLayout {
                session_name,
                window_id,
                layout,
            } => TmuxCommandBuilder::new("select-layout")
                .target(window_target(&session_name, window_id))
                .positional(layout)
                .build(),
            Self::LoadContent { pane_id, filename } => TmuxCommandBuilder::new("send-keys")
                .target(pane_id)
                .positional(load_content_keys(&filename))
                .build(),
        }
    }
}

pub(crate) fn window_target(session_name: &str, window_id: usize) -> String {
    WindowTarget::from_parts(session_name, window_id).into_string()
}

pub(crate) fn pane_target(session_name: &str, window_id: usize, pane_id: usize) -> String {
    PaneTarget::from_parts(session_name, window_id, pane_id).into_string()
}

pub(crate) fn pane_path_keys(path: &Path) -> String {
    format!("builtin cd \"{}\"\nclear\n", path.to_string_lossy())
}

fn load_content_keys(filename: &str) -> String {
    format!("cat   \"{filename}\"\n")
}
