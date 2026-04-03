use retmux::model::{Pane, Session, Size, Tmux, Window};
use retmux::serde_legacy::{self, LegacySnapshotError};

const DEFAULT_FIXTURE_JSON: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/legacy/default_socket/backup_20240101_120000/backup_20240101_120000.json"
);

#[test]
fn helper_semantics_match_legacy_python_model() {
    let mut session = Session::new("work");
    session.windows = vec![
        Window::new("work", 1),
        Window::new("work", 3),
        Window::new("work", 2),
    ];

    let reversed_window_ids = session
        .windows_in_reverse()
        .into_iter()
        .map(|window| window.win_id)
        .collect::<Vec<_>>();
    assert_eq!(reversed_window_ids, vec![3, 2, 1]);

    let mut window = Window::new("work", 7);
    window.panes = vec![
        Pane::new("work", 7, 5),
        Pane::new("work", 7, 1),
        Pane::new("work", 7, 3),
    ];
    assert_eq!(window.min_pane_id(), Some(1));

    let pane = Pane::new("work", 7, 3);
    assert_eq!(pane.idstr(), "work:7.3");
}

#[test]
fn python_fixture_uses_constructor_defaults_when_optional_fields_are_missing() {
    let snapshot = serde_legacy::read_snapshot_file(DEFAULT_FIXTURE_JSON).unwrap();

    assert_eq!(snapshot.tid, "backup_20240101_120000");
    assert_eq!(snapshot.create_time, "2024-01-01 12:00:00");
    assert_eq!(snapshot.sessions.len(), 1);

    let session = &snapshot.sessions[0];
    assert_eq!(session.name, "work");
    assert!(!session.attached);
    assert_eq!(session.size.as_tuple(), Some((120, 40)));
    assert_eq!(session.windows.len(), 1);

    let window = &session.windows[0];
    assert_eq!(window.win_id, 1);
    assert_eq!(window.name, "win1");
    assert!(window.active);
    assert_eq!(window.min_pane_id(), Some(0));

    let pane_ids = window.panes.iter().map(Pane::idstr).collect::<Vec<_>>();
    assert_eq!(pane_ids, vec!["work:1.0", "work:1.1"]);
}

#[test]
fn malformed_legacy_snapshot_is_rejected() {
    let missing_module = r#"
    {
      "__class__": "Tmux",
      "tid": "backup_20240101_120000"
    }
    "#;
    let error = serde_legacy::from_str(missing_module).unwrap_err();
    assert!(matches!(
        error,
        LegacySnapshotError::MissingMarker {
            marker: "__module__",
            ..
        }
    ));

    let invalid_pane_id = r#"
    {
      "__class__": "Tmux",
      "__module__": "tmuxbk.tmux_obj",
      "tid": "backup_20240101_120000",
      "sessions": [
        {
          "__class__": "Session",
          "__module__": "tmuxbk.tmux_obj",
          "name": "work",
          "windows": [
            {
              "__class__": "Window",
              "__module__": "tmuxbk.tmux_obj",
              "sess_name": "work",
              "win_id": 1,
              "panes": [
                {
                  "__class__": "Pane",
                  "__module__": "tmuxbk.tmux_obj",
                  "sess_name": "work",
                  "win_id": 1,
                  "pane_id": "nope"
                }
              ]
            }
          ]
        }
      ]
    }
    "#;

    let error = serde_legacy::from_str(invalid_pane_id).unwrap_err();
    assert!(matches!(
        error,
        LegacySnapshotError::InvalidFieldType {
            field: "pane_id",
            ..
        }
    ));
}

#[test]
fn legacy_numeric_active_flags_are_accepted() {
    let snapshot = r#"
    {
      "__class__": "Tmux",
      "__module__": "tmuxbk.tmux_obj",
      "tid": "backup_20240101_120000",
      "sessions": [
        {
          "__class__": "Session",
          "__module__": "tmuxbk.tmux_obj",
          "name": "work",
          "windows": [
            {
              "__class__": "Window",
              "__module__": "tmuxbk.tmux_obj",
              "sess_name": "work",
              "win_id": 1,
              "active": 1,
              "panes": [
                {
                  "__class__": "Pane",
                  "__module__": "tmuxbk.tmux_obj",
                  "sess_name": "work",
                  "win_id": 1,
                  "pane_id": 0,
                  "active": 0
                }
              ]
            }
          ]
        }
      ]
    }
    "#;

    let decoded = serde_legacy::from_str(snapshot).unwrap();
    let window = &decoded.sessions[0].windows[0];

    assert!(window.active);
    assert!(!window.panes[0].active);
}

#[test]
fn rust_round_trip_preserves_legacy_keys() {
    let mut tmux = Tmux::new("backup_20240403_101010");
    tmux.create_time = "2024-04-03 10:10:10".to_string();

    let mut session = Session::new("work");
    session.attached = true;
    session.size = Size::new(120, 40);

    let mut window = Window::new("work", 2);
    window.name = "editor".to_string();
    window.active = true;
    window.layout = "1900,120x40,0,0,0".to_string();

    let mut pane = Pane::new("work", 2, 0);
    pane.active = true;
    pane.path = "/tmp/work".to_string();
    pane.size = Size::new(120, 40);
    pane.cont_file = "work:2.0".to_string();

    window.panes.push(pane);
    session.windows.push(window);
    tmux.sessions.push(session);

    let encoded = serde_legacy::to_string_pretty(&tmux).unwrap();
    let encoded_value: serde_json::Value = serde_json::from_str(&encoded).unwrap();

    assert_eq!(encoded_value["__class__"], "Tmux");
    assert_eq!(encoded_value["__module__"], "tmuxbk.tmux_obj");
    assert_eq!(encoded_value["tid"], "backup_20240403_101010");
    assert_eq!(encoded_value["sessions"][0]["__class__"], "Session");
    assert_eq!(
        encoded_value["sessions"][0]["windows"][0]["__class__"],
        "Window"
    );
    assert_eq!(encoded_value["sessions"][0]["windows"][0]["name"], "editor");
    assert_eq!(encoded_value["sessions"][0]["windows"][0]["win_id"], 2);
    assert_eq!(
        encoded_value["sessions"][0]["windows"][0]["panes"][0]["__class__"],
        "Pane"
    );
    assert_eq!(
        encoded_value["sessions"][0]["windows"][0]["panes"][0]["pane_id"],
        0
    );
    assert_eq!(
        encoded_value["sessions"][0]["windows"][0]["panes"][0]["cont_file"],
        "work:2.0"
    );
    assert_eq!(
        encoded_value["sessions"][0]["size"],
        serde_json::json!([120, 40])
    );

    let decoded = serde_legacy::from_str(&encoded).unwrap();
    assert_eq!(decoded, tmux);
}
