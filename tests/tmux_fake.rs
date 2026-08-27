mod support;

use std::path::Path;

use remux::tmux_adapter::{TmuxClient, TmuxCommand};

use crate::support::tmux_fake::{FakeTmux, FakeTmuxOutput, FakeTmuxStep};

#[test]
fn fake_tmux_replays_scripted_restore_commands() {
    let fake = FakeTmux::new([
        FakeTmuxStep::ok(
            TmuxCommand::ClearPane {
                pane_id: "demo:1.0".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::SendKeys {
                target: "demo:1.0".to_string(),
                keys: "builtin cd \"/tmp/demo\"\nclear\n".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::ClearPane {
                pane_id: "demo:1.0".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
        FakeTmuxStep::ok(
            TmuxCommand::LoadContent {
                pane_id: "demo:1.0".to_string(),
                filename: "snapshot file.txt".to_string(),
            },
            FakeTmuxOutput::Unit,
        ),
    ]);

    fake.set_pane_path("demo:1.0", Path::new("/tmp/demo"))
        .expect("fake tmux should simulate set_pane_path");
    fake.restore_pane_content("demo:1.0", Path::new("snapshot file.txt"))
        .expect("fake tmux should simulate content restore");

    assert_eq!(fake.remaining_steps(), 0);
    assert_eq!(
        fake.rendered_commands(),
        vec![
            vec!["tmux", "clear-history", "-tdemo:1.0"],
            vec![
                "tmux",
                "send-keys",
                "-tdemo:1.0",
                "builtin cd \"/tmp/demo\"\nclear\n",
            ],
            vec!["tmux", "clear-history", "-tdemo:1.0"],
            vec![
                "tmux",
                "send-keys",
                "-tdemo:1.0",
                "cat   \"snapshot file.txt\"\n",
            ],
        ]
    );
}

#[test]
fn fake_tmux_can_simulate_backup_queries() {
    let fake = FakeTmux::new([
        FakeTmuxStep::ok(TmuxCommand::ListSessions, FakeTmuxOutput::Bool(true)),
        FakeTmuxStep::ok(
            TmuxCommand::ListSessions,
            FakeTmuxOutput::Lines(vec!["work:=:(120,40):=:0".to_string()]),
        ),
        FakeTmuxStep::ok(
            TmuxCommand::CapturePane {
                pane_id: "work:1.0".to_string(),
                include_escape: true,
            },
            FakeTmuxOutput::Bytes(b"pane0\n".to_vec()),
        ),
    ]);

    assert!(fake.has_server().expect("fake has_server should succeed"));
    assert_eq!(
        fake.list_sessions()
            .expect("fake list_sessions should succeed"),
        vec!["work:=:(120,40):=:0".to_string()]
    );
    assert_eq!(
        fake.capture_pane_bytes("work:1.0")
            .expect("fake capture should succeed"),
        b"pane0\n".to_vec()
    );
    assert_eq!(fake.remaining_steps(), 0);
}
