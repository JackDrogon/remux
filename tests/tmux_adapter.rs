use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::config::{AppState, ExecutionOptions};
use remux::tmux_adapter::{TmuxCommand, TmuxRuntimeOptions};
use remux::{Code, Tmux as TmuxError};

#[test]
fn socket_prefix_is_inserted() {
    let temp_home = TempHome::new("socket-prefix");
    let mut config = AppState::load_from_home(temp_home.path())
        .expect("config bootstrap should succeed for socket-prefix test");

    let default_adapter = TmuxRuntimeOptions::new(&config.config().tmux.binary)
        .socket_name(config.socket_name())
        .content_with_escape(config.config().capture.with_escape)
        .build_adapter();
    assert_eq!(
        default_adapter.render_command(TmuxCommand::HasSession {
            session_name: "demo".to_string(),
        }),
        vec!["tmux", "has-session", "-tdemo"]
    );

    config.set_execution_options(ExecutionOptions::with_socket_name(Some("sockA")));
    let socket_adapter = TmuxRuntimeOptions::new(&config.config().tmux.binary)
        .socket_name(config.socket_name())
        .content_with_escape(config.config().capture.with_escape)
        .build_adapter();
    assert_eq!(
        socket_adapter.render_command(TmuxCommand::HasSession {
            session_name: "demo".to_string(),
        }),
        vec!["tmux", "-L", "sockA", "has-session", "-tdemo"]
    );
}

#[test]
fn rendered_command_templates_match_tmux_shapes() {
    let adapter = TmuxRuntimeOptions::new("tmux")
        .content_with_escape(true)
        .build_adapter();

    assert_eq!(
        adapter.render_command(TmuxCommand::ListSessions),
        vec![
            "tmux",
            "list-sessions",
            "-F#S:=:(#{window_width},#{window_height}):=:#{session_attached}"
        ]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::ListWindows {
            session_name: "demo".to_string(),
        }),
        vec![
            "tmux",
            "list-windows",
            "-F#{window_index}:=:#{window_name}:=:#{window_active}:=:#{window_layout}",
            "-tdemo"
        ]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::ListPanes {
            session_name: "demo".to_string(),
            window_index: 2,
        }),
        vec![
            "tmux",
            "list-panes",
            "-tdemo:2",
            "-F#{pane_index}:=:(#{pane_width},#{pane_height}):=:#{pane_current_path}:=:#{pane_active}:=:#{pane_current_command}:=:#{pane_pid}"
        ]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::CreateSession {
            session_name: "demo".to_string(),
            width: 200,
            height: 60,
        }),
        vec!["tmux", "new-session", "-d", "-sdemo", "-x200", "-y60"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::KillSession {
            session_name: "demo".to_string(),
        }),
        vec!["tmux", "kill-session", "-tdemo"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::ShowOption {
            option: "base-index".to_string(),
        }),
        vec!["tmux", "show-options", "-gv", "base-index"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::SplitWindow {
            session_name: "demo".to_string(),
            window_id: 1,
            pane_min_id: 0,
        }),
        vec!["tmux", "split-window", "-d", "-l3", "-tdemo:1.0"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::SelectLayout {
            session_name: "demo".to_string(),
            window_id: 1,
            layout: "main-vertical".to_string(),
        }),
        vec!["tmux", "select-layout", "-tdemo:1", "main-vertical"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::MoveWindow {
            source: "demo:5".to_string(),
            target: "demo:1".to_string(),
        }),
        vec!["tmux", "move-window", "-sdemo:5", "-tdemo:1"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::RenameWindow {
            session_name: "demo".to_string(),
            window_id: 1,
            name: "editor".to_string(),
        }),
        vec!["tmux", "rename-window", "-tdemo:1", "editor"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::NewEmptyWindow {
            session_name: "demo".to_string(),
            base_index: 1,
        }),
        vec!["tmux", "new-window", "-d", "-tdemo:1"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::SelectWindow {
            session_name: "demo".to_string(),
            window_id: 1,
        }),
        vec!["tmux", "select-window", "-tdemo:1"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::CapturePane {
            pane_id: "demo:1.0".to_string(),
            include_escape: true,
        }),
        vec!["tmux", "capture-pane", "-ep", "-S-100000", "-tdemo:1.0"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::ClearPane {
            pane_id: "demo:1.0".to_string(),
        }),
        vec!["tmux", "clear-history", "-tdemo:1.0"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::CapturePane {
            pane_id: "demo:1.0".to_string(),
            include_escape: false,
        }),
        vec!["tmux", "capture-pane", "-p", "-S-100000", "-tdemo:1.0"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::SendKeys {
            target: "demo:1.0".to_string(),
            keys: "echo hello\n".to_string(),
        }),
        vec!["tmux", "send-keys", "-tdemo:1.0", "echo hello\n"]
    );
    assert_eq!(
        adapter.render_command(TmuxCommand::LoadContent {
            pane_id: "demo:1.0".to_string(),
            filename: "snapshot file.txt".to_string(),
        }),
        vec![
            "tmux",
            "send-keys",
            "-tdemo:1.0",
            "cat   \"snapshot file.txt\"\n"
        ]
    );
}

#[test]
fn missing_tmux_binary_returns_typed_error() {
    let missing_binary = unique_missing_binary();
    let adapter = TmuxRuntimeOptions::new(missing_binary.clone())
        .content_with_escape(true)
        .build_adapter();

    let error = adapter.list_sessions().unwrap_err();

    match error.code() {
        Code::Tmux(TmuxError::BinaryNotFound { command, .. }) => {
            assert_eq!(
                command,
                &vec![
                    missing_binary,
                    "list-sessions".to_string(),
                    "-F#S:=:(#{window_width},#{window_height}):=:#{session_attached}".to_string(),
                ]
            );
        }
        other => panic!("expected BinaryNotFound error, got {other:?}"),
    }
}

fn unique_missing_binary() -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "remux-missing-tmux-{}-{unique}",
        std::process::id()
    ));
    if path.exists() {
        fs::remove_file(&path).expect("stale missing-binary test path should be removable");
    }
    path.to_string_lossy().into_owned()
}

struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-tmux-adapter-{label}-{}-{unique}",
            std::process::id()
        ));

        if path.exists() {
            fs::remove_dir_all(&path).expect("should clear stale temp HOME");
        }
        fs::create_dir_all(&path).expect("should create temp HOME");

        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
