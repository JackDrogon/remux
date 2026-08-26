use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::model::{Pane, Process, Session, Size, Tmux, Window};
use remux::storage::{self, SnapshotError};

mod support;

#[test]
fn write_rejects_missing_pane_bytes() {
    let temp = TempDir::new("missing-pane-bytes");
    let mut tmux = Tmux::new("backup_20240101_120000");
    tmux.create_time = "2024-01-01 12:00:00".to_string();

    let mut session = Session::new("work");
    session.size = Size::new(120, 40);
    let mut window = Window::new("work", 1);
    window.name = "editor".to_string();
    window.active = true;
    window.layout = "1900,120x40,0,0,0".to_string();
    let mut pane = Pane::new("work", 1, 0);
    pane.active = true;
    pane.path = "/tmp/work".to_string();
    pane.size = Size::new(120, 40);
    window.panes.push(pane);
    session.windows.push(window);
    tmux.sessions.push(session);

    let error = storage::write_snapshot_dir(
        temp.path().join("backup_20240101_120000").as_path(),
        &tmux,
        &BTreeMap::new(),
    )
    .expect_err("missing pane bytes should fail");
    assert!(matches!(error, SnapshotError::MissingPaneBytes { .. }));
}

#[test]
fn write_rejects_duplicate_pane_ids() {
    let temp = TempDir::new("duplicate-pane-id");
    let mut tmux = Tmux::new("backup_20240101_120000");
    tmux.create_time = "2024-01-01 12:00:00".to_string();

    let mut session = Session::new("work");
    let mut window = Window::new("work", 1);
    let pane_a = Pane::new("work", 1, 0);
    let pane_b = Pane::new("work", 1, 0);
    window.panes = vec![pane_a, pane_b];
    session.windows.push(window);
    tmux.sessions.push(session);

    let mut pane_contents = BTreeMap::new();
    pane_contents.insert("work:1.0".to_string(), b"content\n".to_vec());

    let error = storage::write_snapshot_dir(
        temp.path().join("backup_20240101_120000").as_path(),
        &tmux,
        &pane_contents,
    )
    .expect_err("duplicate pane ids should fail");
    assert!(matches!(error, SnapshotError::DuplicatePaneId { .. }));
}

#[test]
fn read_rejects_summary_manifest_hash_mismatch() {
    let temp = write_basic_snapshot("hash-mismatch");
    let backup_dir = temp.path().join("backup_20240101_120000");
    let summary_path = backup_dir.join("summary.json");

    let mut summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).expect("summary should exist"))
            .expect("summary json should parse");
    summary["manifest_sha256"] = serde_json::Value::String("0".repeat(64));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary rewrite should serialize"),
    )
    .expect("tampered summary should write");

    let error = storage::read_snapshot_dir(&backup_dir).expect_err("hash mismatch should fail");
    assert!(matches!(
        error,
        SnapshotError::SummaryManifestMismatch { .. }
    ));
}

#[test]
fn read_rejects_relative_path_escape() {
    let temp = write_basic_snapshot("relative-path-escape");
    let backup_dir = temp.path().join("backup_20240101_120000");
    let manifest_path = backup_dir.join("manifest.json");

    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("manifest should exist"))
            .expect("manifest json should parse");
    manifest["pane_table"]["work:1.0"]["relative_path"] =
        serde_json::Value::String("../outside.txt".to_string());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest rewrite should serialize"),
    )
    .expect("tampered manifest should write");

    let summary_path = backup_dir.join("summary.json");
    let mut summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).expect("summary should exist"))
            .expect("summary json should parse");
    let manifest_bytes = fs::read(&manifest_path).expect("manifest bytes should read");
    summary["manifest_sha256"] = serde_json::Value::String(sha256_hex(&manifest_bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary rewrite should serialize"),
    )
    .expect("summary rewrite should succeed");

    let error = storage::read_snapshot_dir(&backup_dir).expect_err("path escape should fail");
    assert!(matches!(error, SnapshotError::InvalidRelativePath { .. }));
}

#[test]
fn summary_reader_rejects_unsupported_major_version() {
    let temp = write_basic_snapshot("unsupported-major");
    let backup_dir = temp.path().join("backup_20240101_120000");
    let summary_path = backup_dir.join("summary.json");

    let mut summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).expect("summary should exist"))
            .expect("summary json should parse");
    summary["schema_version"]["major"] = serde_json::Value::Number(99u64.into());
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary rewrite should serialize"),
    )
    .expect("summary rewrite should succeed");

    let error = storage::read_snapshot_summary_dir(&backup_dir)
        .expect_err("unsupported major version should fail");
    assert!(matches!(error, SnapshotError::UnsupportedVersion { .. }));
}

#[test]
fn read_rejects_manifest_with_missing_pane_table_entry() {
    let temp = write_basic_snapshot("missing-pane-table");
    let backup_dir = temp.path().join("backup_20240101_120000");
    let manifest_path = backup_dir.join("manifest.json");

    let mut manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).expect("manifest should exist"))
            .expect("manifest json should parse");
    manifest["pane_table"] = serde_json::json!({});
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("manifest rewrite should serialize"),
    )
    .expect("manifest rewrite should succeed");

    let summary_path = backup_dir.join("summary.json");
    let mut summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&summary_path).expect("summary should exist"))
            .expect("summary json should parse");
    let manifest_bytes = fs::read(&manifest_path).expect("manifest bytes should read");
    summary["manifest_sha256"] = serde_json::Value::String(sha256_hex(&manifest_bytes));
    fs::write(
        &summary_path,
        serde_json::to_vec_pretty(&summary).expect("summary rewrite should serialize"),
    )
    .expect("summary rewrite should succeed");

    let error =
        storage::read_snapshot_dir(&backup_dir).expect_err("missing pane table entry should fail");
    assert!(matches!(error, SnapshotError::InvalidManifest { .. }));
}

#[test]
fn write_and_read_preserves_command_tree() {
    let temp = TempDir::new("pane-command");
    let backup_dir = temp.path().join("backup_20240101_120000");
    let (mut tmux, pane_contents) = support::single_window_tmux(
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );
    tmux.sessions[0].windows[0].panes[0].command_tree = Some(Process {
        name: "zsh".to_string(),
        argv: vec!["zsh".to_string()],
        pid: 1000,
        foreground: false,
        children: vec![
            Process {
                name: "sleep".to_string(),
                argv: vec!["sleep".to_string(), "999".to_string()],
                pid: 1001,
                foreground: false,
                children: Vec::new(),
            },
            Process {
                name: "vim".to_string(),
                argv: vec!["vim".to_string(), "/tmp/notes.md".to_string()],
                pid: 4242,
                foreground: true,
                children: Vec::new(),
            },
        ],
    });

    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("snapshot directory should be written");
    let loaded =
        storage::read_snapshot_dir(&backup_dir).expect("snapshot with command tree should load");
    let command_tree = loaded.tmux.sessions[0].windows[0].panes[0]
        .command_tree
        .as_ref()
        .expect("command tree should survive the snapshot round trip");

    assert_eq!(command_tree.pid, 1000);
    assert_eq!(command_tree.name, "zsh");
    assert!(!command_tree.foreground);
    assert_eq!(
        command_tree
            .children
            .iter()
            .map(|child| child.pid)
            .collect::<Vec<_>>(),
        vec![1001, 4242]
    );
    assert!(!command_tree.children[0].foreground);
    assert!(command_tree.children[1].foreground);
    assert_eq!(
        command_tree.children[1].argv,
        vec!["vim".to_string(), "/tmp/notes.md".to_string()]
    );

    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(backup_dir.join("manifest.json"))
            .expect("written manifest should be readable"),
    )
    .expect("written manifest should parse");
    assert_pane_json_persists_only_command_tree(&manifest["sessions"][0]["windows"][0]["panes"][0]);
}

#[test]
fn read_accepts_schema_1_0_snapshot_without_command_tree() {
    let loaded = storage::read_snapshot_dir(&support::schema_1_0_snapshot_dir())
        .expect("frozen schema 1.0 snapshot should still load");

    assert_eq!(loaded.tmux.backup_id, "backup_20240101_120000");
    assert!(
        loaded.tmux.sessions[0].windows[0].panes[0]
            .command_tree
            .is_none(),
        "schema 1.0 panes have no command_tree field and must load as None"
    );
}

fn write_basic_snapshot(label: &str) -> TempDir {
    let temp = TempDir::new(label);
    let backup_dir = temp.path().join("backup_20240101_120000");
    let (tmux, pane_contents) = support::single_window_tmux(
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );
    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("snapshot directory should be written");
    temp
}

fn assert_pane_json_persists_only_command_tree(pane: &serde_json::Value) {
    let object = pane
        .as_object()
        .expect("written pane should be a JSON object");
    assert!(
        !object.contains_key("current_command"),
        "pane must not persist a duplicated current_command, keys were: {:?}",
        object.keys().collect::<Vec<_>>()
    );
    let command_tree = object
        .get("command_tree")
        .expect("written pane should include command_tree");
    assert_command_tree_json_fields(command_tree);
}

fn assert_command_tree_json_fields(value: &serde_json::Value) {
    let object = value
        .as_object()
        .expect("command_tree process should be a JSON object");
    let mut keys = object.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        ["argv", "children", "foreground", "name", "pid"],
        "command_tree must persist exactly name, argv, pid, foreground, and children"
    );
    for child in object["children"]
        .as_array()
        .expect("command_tree children should be an array")
    {
        assert_command_tree_json_fields(child);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-snapshot-contract-{label}-{}-{unique}",
            std::process::id()
        ));

        if path.exists() {
            fs::remove_dir_all(&path).expect("stale temp dir should be removable");
        }
        fs::create_dir_all(&path).expect("temp dir should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
