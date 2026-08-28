use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use remux::model::{Pane, Session, Size, Tmux, Window};
use remux::tmux_adapter::TmuxCommand;

use self::tmux_fake::{FakeTmux, FakeTmuxOutput, FakeTmuxStep};

pub mod tmux_fake;

#[allow(dead_code)]
pub fn schema_1_0_snapshot_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/legacy/snapshot_schema_1_0/backup_20240101_120000")
}

#[allow(dead_code)]
pub fn single_window_tmux(
    backup_id: &str,
    session_name: &str,
    create_time: &str,
    pane_paths: &[&str],
) -> (Tmux, BTreeMap<String, Vec<u8>>) {
    let mut tmux = Tmux::new(backup_id);
    tmux.create_time = create_time.to_string();

    let mut session = Session::new(session_name);
    session.size = Size::new(120, 40);

    let mut window = Window::new(session_name, 1);
    window.name = "editor".to_string();
    window.active = true;
    window.layout = "1900,120x40,0,0,0".to_string();

    let mut pane_contents = BTreeMap::new();
    for (index, pane_path) in pane_paths.iter().enumerate() {
        let mut pane = Pane::new(session_name, 1, index as u32);
        pane.active = index == 0;
        pane.size = Size::new(120, 40);
        pane.path = (*pane_path).to_string();
        pane_contents.insert(
            pane.pane_target().into_string(),
            format!("content for {pane_path}\n").into_bytes(),
        );
        window.panes.push(pane);
    }

    session.windows.push(window);
    tmux.sessions.push(session);
    (tmux, pane_contents)
}

#[allow(dead_code)]
pub fn single_window_restore_fake(backup_dir: &Path) -> FakeTmux {
    FakeTmux::new([
        FakeTmuxStep::ok(TmuxCommand::ListSessions, FakeTmuxOutput::Bool(true)),
        FakeTmuxStep::ok(
            TmuxCommand::HasSession {
                session_name: "work".to_string(),
            },
            FakeTmuxOutput::Bool(false),
        ),
        FakeTmuxStep::ok(TmuxCommand::ListSessions, FakeTmuxOutput::Bool(true)),
        FakeTmuxStep::ok(
            TmuxCommand::ShowOption {
                option: "base-index".to_string(),
            },
            FakeTmuxOutput::Text("1".to_string()),
        ),
        FakeTmuxStep::ok(
            TmuxCommand::CreateSession {
                session_name: "work".to_string(),
                width: 120,
                height: 40,
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::RenameWindow {
                session_name: "work".to_string(),
                window_id: 1,
                name: "editor".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::SelectWindow {
                session_name: "work".to_string(),
                window_id: 1,
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::ClearPane {
                pane_id: "work:1.0".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::SendKeys {
                target: "work:1.0".to_string(),
                keys: "builtin cd \"/tmp/work\"\nclear\n".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::ClearPane {
                pane_id: "work:1.0".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::LoadContent {
                pane_id: "work:1.0".to_string(),
                filename: backup_dir
                    .join("panes/work:1.0.txt")
                    .to_string_lossy()
                    .into_owned(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::SelectLayout {
                session_name: "work".to_string(),
                window_id: 1,
                layout: "1900,120x40,0,0,0".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
    ])
}
