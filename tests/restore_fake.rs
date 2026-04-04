mod support;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use remux::restore::restore_from_path_with_adapter;
use remux::storage;
use remux::tmux::TmuxCommand;

use crate::support::single_window_tmux;
use crate::support::tmux_fake::{FakeTmux, FakeTmuxOutput, FakeTmuxStep};

#[test]
fn restore_flow_accepts_fake_tmux_client() {
    let sandbox = RestoreFakeSandbox::new("restore-flow");
    let backup_name = "backup_20240101_120000";
    let (tmux, pane_contents) =
        single_window_tmux(backup_name, "work", "2024-01-01 12:00:00", &["/tmp/work"]);
    let backup_dir = sandbox.backup_root.join(backup_name);
    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("snapshot fixture should be written");

    let fake = FakeTmux::new([
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
    ]);

    restore_from_path_with_adapter(&sandbox.backup_root, &fake, backup_name)
        .expect("restore should succeed with fake tmux client");

    assert_eq!(fake.remaining_steps(), 0);
}

struct RestoreFakeSandbox {
    root: PathBuf,
    backup_root: PathBuf,
}

impl RestoreFakeSandbox {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "remux-restore-fake-{label}-{}-{unique}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale restore fake sandbox should be removable");
        }
        let backup_root = root.join("backups");
        fs::create_dir_all(&backup_root).expect("restore fake backup root should be created");
        Self { root, backup_root }
    }
}

impl Drop for RestoreFakeSandbox {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
