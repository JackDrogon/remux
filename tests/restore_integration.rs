use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use remux::actions::restore::{resolve_backup_name, restore_from_path_with_adapter};
use remux::model::{Pane, Session, Size, Tmux, Window};
use remux::storage;
use remux::tmux_adapter::TmuxAdapter;

mod support;

#[test]
fn restores_latest_backup_when_name_missing() {
    let sandbox = RestoreSandbox::new("latest-fallback");
    let backup_root = sandbox.backup_root();

    let (older_tmux, older_panes) = support::single_window_tmux(
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/default-work", "/tmp/default-side"],
    );
    storage::write_snapshot_dir(
        &backup_root.join("backup_20240101_120000"),
        &older_tmux,
        &older_panes,
    )
    .expect("older snapshot should be written");
    write_backup(
        &backup_root,
        "backup_20240101_130000",
        latest_snapshot_tmux("backup_20240101_130000"),
        &[
            ("alpha:3.0", "session alpha main pane\n"),
            ("alpha:3.1", "session alpha side pane\n"),
            ("alpha:1.0", "session alpha shell\n"),
        ],
    );
    set_backup_mtime(&backup_root.join("backup_20240101_120000"), 1);
    set_backup_mtime(&backup_root.join("backup_20240101_130000"), 2);

    let selected = resolve_backup_name(&backup_root, None).expect("latest backup should resolve");
    assert_eq!(selected, "backup_20240101_130000");

    let adapter = sandbox.adapter_owned();
    restore_from_path_with_adapter(&backup_root, &adapter, &selected)
        .expect("restore should succeed for latest backup");

    let log = sandbox.read_log();
    assert_contains(&log, "new-session -d -sremux_dummy_");
    assert_contains(&log, "kill-session -tremux_dummy_");
    assert_contains(&log, "new-session -d -salpha -x140 -y45");
    assert_contains(&log, "rename-window -talpha:3 editor");
    assert_contains(&log, "rename-window -talpha:1 shell");
    assert_contains(&log, "send-keys -talpha:3.0 builtin cd \"/tmp/alpha/main\"");
    assert_contains(&log, "send-keys -talpha:3.0 cat   \"");
    assert_contains(
        &log,
        "select-layout -talpha:3 a1b2,140x45,0,0{70x45,0,0,0,69x45,71,0,1}",
    );

    let rename_editor = log
        .find("rename-window -talpha:3 editor")
        .expect("editor window rename should be logged");
    let rename_shell = log
        .find("rename-window -talpha:1 shell")
        .expect("shell window rename should be logged");
    assert!(
        rename_editor < rename_shell,
        "windows should restore in reverse order; log was:\n{log}"
    );
}

#[test]
fn skips_conflicting_session_names() {
    let sandbox = RestoreSandbox::new("conflict-skip");
    let backup_root = sandbox.backup_root();

    write_backup(
        &backup_root,
        "backup_20240102_120000",
        conflict_snapshot_tmux("backup_20240102_120000"),
        &[
            ("existing:1.0", "existing pane\n"),
            ("fresh:2.0", "fresh pane\n"),
        ],
    );
    sandbox.seed_existing_sessions(&["existing"]);

    let adapter = sandbox.adapter_owned();
    restore_from_path_with_adapter(&backup_root, &adapter, "backup_20240102_120000")
        .expect("restore should skip conflicting sessions and continue");

    let log = sandbox.read_log();
    assert!(
        !log.contains("-sexisting"),
        "conflicting session must not be recreated: {log}"
    );
    assert_contains(&log, "has-session -texisting");
    assert_contains(&log, "new-session -d -sfresh -x90 -y28");
    assert_contains(&log, "rename-window -tfresh:2 fresh-window");
}

#[test]
fn malformed_backup_fails_fast() {
    let sandbox = RestoreSandbox::new("malformed-fast-fail");
    let backup_root = sandbox.backup_root();
    let backup_dir = backup_root.join("backup_bad");
    fs::create_dir_all(&backup_dir).expect("should create malformed backup dir");
    fs::write(backup_dir.join("summary.json"), "{\"backup_id\":true}\n")
        .expect("should write malformed summary");

    let adapter = sandbox.adapter_owned();
    let error = restore_from_path_with_adapter(&backup_root, &adapter, "backup_bad")
        .expect_err("malformed snapshot must fail");
    let message = error.to_string();

    assert!(
        message.contains("failed to load snapshot") && message.contains("JSON error"),
        "unexpected malformed restore error: {message}"
    );

    let log = sandbox.read_log();
    assert!(
        log.trim().is_empty(),
        "malformed snapshot should fail before tmux mutations; log was:\n{log}"
    );
}

#[test]
fn missing_pane_file_fails_fast_before_tmux_mutation() {
    let sandbox = RestoreSandbox::new("missing-pane-fast-fail");
    let backup_root = sandbox.backup_root();

    write_backup(
        &backup_root,
        "backup_missing_pane",
        latest_snapshot_tmux("backup_missing_pane"),
        &[
            ("alpha:3.0", "session alpha main pane\n"),
            ("alpha:3.1", "session alpha side pane\n"),
            ("alpha:1.0", "session alpha shell\n"),
        ],
    );
    fs::remove_file(
        backup_root
            .join("backup_missing_pane")
            .join("panes")
            .join("alpha:1.0.txt"),
    )
    .expect("one pane file should be removed to simulate corruption");

    let adapter = sandbox.adapter_owned();
    let error = restore_from_path_with_adapter(&backup_root, &adapter, "backup_missing_pane")
        .expect_err("missing pane content must fail before restore starts");
    let message = error.to_string();

    assert!(
        message.contains("missing pane content for alpha:1.0"),
        "unexpected missing-pane restore error: {message}"
    );

    let log = sandbox.read_log();
    assert_contains(
        &log,
        "list-sessions -F#S:=:(#{window_width},#{window_height}):=:#{session_attached}",
    );
    assert!(
        !log.contains("new-session -d -sremux_dummy_"),
        "missing pane content should fail before creating a dummy session; log was:\n{log}"
    );
    assert!(
        !log.contains("new-session -d -salpha"),
        "missing pane content should fail before creating restored sessions; log was:\n{log}"
    );
    assert!(
        !log.contains("rename-window -talpha:"),
        "missing pane content should fail before window mutations; log was:\n{log}"
    );
}

#[test]
fn tampered_pane_file_fails_fast_before_tmux_mutation() {
    let sandbox = RestoreSandbox::new("tampered-pane-fast-fail");
    let backup_root = sandbox.backup_root();

    write_backup(
        &backup_root,
        "backup_tampered_pane",
        latest_snapshot_tmux("backup_tampered_pane"),
        &[
            ("alpha:3.0", "session alpha main pane\n"),
            ("alpha:3.1", "session alpha side pane\n"),
            ("alpha:1.0", "session alpha shell\n"),
        ],
    );
    fs::write(
        backup_root
            .join("backup_tampered_pane")
            .join("panes")
            .join("alpha:1.0.txt"),
        "tampered pane\n",
    )
    .expect("one pane file should be tampered to simulate checksum mismatch");

    let adapter = sandbox.adapter_owned();
    let error = restore_from_path_with_adapter(&backup_root, &adapter, "backup_tampered_pane")
        .expect_err("tampered pane content must fail before restore starts");
    let message = error.to_string();

    assert!(
        message.contains("invalid pane content for alpha:1.0")
            && (message.contains("expected sha256") || message.contains("expected 20 bytes")),
        "unexpected tampered-pane restore error: {message}"
    );

    let log = sandbox.read_log();
    assert_contains(
        &log,
        "list-sessions -F#S:=:(#{window_width},#{window_height}):=:#{session_attached}",
    );
    assert!(
        !log.contains("new-session -d -sremux_dummy_"),
        "tampered pane content should fail before creating a dummy session; log was:\n{log}"
    );
    assert!(
        !log.contains("new-session -d -salpha"),
        "tampered pane content should fail before creating restored sessions; log was:\n{log}"
    );
}

#[test]
fn named_restore_resolution_trims_and_rejects_invalid_names() {
    let sandbox = RestoreSandbox::new("normalized-restore-name");
    let backup_root = sandbox.backup_root();

    write_backup(
        &backup_root,
        "backup_trimmed",
        latest_snapshot_tmux("backup_trimmed"),
        &[
            ("alpha:3.0", "session alpha main pane\n"),
            ("alpha:3.1", "session alpha side pane\n"),
            ("alpha:1.0", "session alpha shell\n"),
        ],
    );

    let resolved = resolve_backup_name(&backup_root, Some("  backup_trimmed  "))
        .expect("named restore should trim surrounding whitespace");
    assert_eq!(resolved, "backup_trimmed");

    for invalid_name in [
        "",
        "   ",
        "..",
        "nested/name",
        "nested\\name",
        "/tmp/backup",
    ] {
        let error = resolve_backup_name(&backup_root, Some(invalid_name))
            .expect_err("invalid restore target should fail deterministically");
        let message = error.to_string();
        assert!(
            message.contains("invalid backup name"),
            "expected invalid restore target error for {invalid_name:?}, got: {message}"
        );
    }
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected log to contain {needle:?}, got:\n{haystack}"
    );
}

fn write_backup(backup_root: &Path, backup_name: &str, tmux: Tmux, pane_files: &[(&str, &str)]) {
    let backup_dir = backup_root.join(backup_name);
    fs::create_dir_all(&backup_dir).expect("should create backup directory");
    let pane_contents = pane_files
        .iter()
        .map(|(pane_id, content)| (pane_id.to_string(), content.as_bytes().to_vec()))
        .collect::<std::collections::BTreeMap<_, _>>();
    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("should write snapshot directory");
}

fn set_backup_mtime(path: &Path, seconds: u64) {
    let timestamp = UNIX_EPOCH + Duration::from_secs(1_700_000_000 + seconds);
    let filetime = format_time_for_touch(timestamp);
    std::process::Command::new("touch")
        .arg("-d")
        .arg(filetime)
        .arg(path)
        .status()
        .expect("touch should run for mtime update");
}

fn format_time_for_touch(timestamp: SystemTime) -> String {
    let seconds = timestamp
        .duration_since(UNIX_EPOCH)
        .expect("timestamp should be after UNIX_EPOCH")
        .as_secs();
    format!("@{seconds}")
}

fn latest_snapshot_tmux(backup_id: &str) -> Tmux {
    let mut tmux = Tmux::new(backup_id);
    tmux.create_time = "2024-01-01 13:00:00".to_string();

    let mut session = Session::new("alpha");
    session.size = Size::new(140, 45);

    let mut shell_window = Window::new("alpha", 1);
    shell_window.name = "shell".to_string();
    shell_window.layout = "1900,140x45,0,0,0".to_string();
    let mut shell_pane = Pane::new("alpha", 1, 0);
    shell_pane.active = true;
    shell_pane.path = "/tmp/alpha/shell".to_string();
    shell_pane.size = Size::new(140, 45);
    shell_window.panes.push(shell_pane);

    let mut editor_window = Window::new("alpha", 3);
    editor_window.name = "editor".to_string();
    editor_window.active = true;
    editor_window.layout = "a1b2,140x45,0,0{70x45,0,0,0,69x45,71,0,1}".to_string();
    let mut main_pane = Pane::new("alpha", 3, 0);
    main_pane.active = true;
    main_pane.path = "/tmp/alpha/main".to_string();
    main_pane.size = Size::new(70, 45);
    let mut side_pane = Pane::new("alpha", 3, 1);
    side_pane.path = "/tmp/alpha/side".to_string();
    side_pane.size = Size::new(69, 45);
    editor_window.panes.push(main_pane);
    editor_window.panes.push(side_pane);

    session.windows.push(shell_window);
    session.windows.push(editor_window);
    tmux.sessions.push(session);
    tmux
}

fn conflict_snapshot_tmux(backup_id: &str) -> Tmux {
    let mut tmux = Tmux::new(backup_id);
    tmux.create_time = "2024-01-02 12:00:00".to_string();

    let mut existing = Session::new("existing");
    existing.size = Size::new(80, 24);
    let mut existing_window = Window::new("existing", 1);
    existing_window.name = "existing-window".to_string();
    existing_window.active = true;
    existing_window.layout = "cafe,80x24,0,0,0".to_string();
    let mut existing_pane = Pane::new("existing", 1, 0);
    existing_pane.active = true;
    existing_pane.path = "/tmp/existing".to_string();
    existing_pane.size = Size::new(80, 24);
    existing_window.panes.push(existing_pane);
    existing.windows.push(existing_window);

    let mut fresh = Session::new("fresh");
    fresh.size = Size::new(90, 28);
    let mut fresh_window = Window::new("fresh", 2);
    fresh_window.name = "fresh-window".to_string();
    fresh_window.active = true;
    fresh_window.layout = "dead,90x28,0,0,0".to_string();
    let mut fresh_pane = Pane::new("fresh", 2, 0);
    fresh_pane.active = true;
    fresh_pane.path = "/tmp/fresh".to_string();
    fresh_pane.size = Size::new(90, 28);
    fresh_window.panes.push(fresh_pane);
    fresh.windows.push(fresh_window);

    tmux.sessions.push(existing);
    tmux.sessions.push(fresh);
    tmux
}

struct RestoreSandbox {
    root: PathBuf,
    fake_tmux_path: PathBuf,
    log_path: PathBuf,
    state_path: PathBuf,
}

impl RestoreSandbox {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "remux-restore-integration-{label}-{}-{unique}",
            std::process::id()
        ));

        if root.exists() {
            fs::remove_dir_all(&root).expect("stale restore sandbox should be removable");
        }
        fs::create_dir_all(&root).expect("restore sandbox root should be created");

        let log_path = root.join("tmux.log");
        let state_path = root.join("tmux.state");
        let fake_tmux_path = root.join("fake_tmux.sh");

        fs::write(&log_path, "").expect("should create fake tmux log file");
        fs::write(&state_path, "server=0\nsessions=\n").expect("should seed fake tmux state");
        fs::write(&fake_tmux_path, fake_tmux_script()).expect("should write fake tmux script");
        let mut permissions = fs::metadata(&fake_tmux_path)
            .expect("fake tmux script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_tmux_path, permissions)
            .expect("fake tmux script should be executable");

        let backup_root = root.join("backups");
        fs::create_dir_all(&backup_root).expect("backup root should be created");

        Self {
            root,
            fake_tmux_path,
            log_path,
            state_path,
        }
    }

    fn backup_root(&self) -> PathBuf {
        self.root.join("backups")
    }

    fn adapter_owned(&self) -> TmuxAdapter {
        TmuxAdapter::from_prefix(
            vec![self.fake_tmux_path.to_string_lossy().into_owned()],
            true,
        )
    }

    fn seed_existing_sessions(&self, sessions: &[&str]) {
        let body = format!("server=1\nsessions={}\n", sessions.join(","));
        fs::write(&self.state_path, body).expect("should seed fake tmux sessions");
    }

    fn read_log(&self) -> String {
        fs::read_to_string(&self.log_path).expect("fake tmux log should be readable")
    }
}

impl Drop for RestoreSandbox {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn fake_tmux_script() -> &'static str {
    r#"#!/bin/zsh
set -eu

script_dir=${0:A:h}
log_path="$script_dir/tmux.log"
state_path="$script_dir/tmux.state"

if [[ ! -f "$state_path" ]]; then
  print -r -- 'server=0' > "$state_path"
  print -r -- 'sessions=' >> "$state_path"
fi

server=0
sessions_csv=''
while IFS='=' read -r key value; do
  case "$key" in
    server) server="$value" ;;
    sessions) sessions_csv="$value" ;;
  esac
done < "$state_path"

typeset -a sessions
if [[ -n "$sessions_csv" ]]; then
  sessions=(${(s:,:)sessions_csv})
else
  sessions=()
fi

save_state() {
  local joined="${(j:,:)sessions}"
  print -r -- "server=$server" > "$state_path"
  print -r -- "sessions=$joined" >> "$state_path"
}

append_log() {
  print -r -- "$*" >> "$log_path"
}

append_log "$*"

command_name="$1"
shift || true

case "$command_name" in
  list-sessions)
    if [[ "$server" == "1" ]]; then
      exit 0
    fi
    exit 1
    ;;
  has-session)
    target="${1#-t}"
    for session_name in $sessions; do
      if [[ "$session_name" == "$target" ]]; then
        exit 0
      fi
    done
    exit 1
    ;;
  show-options)
    print -r -- '1'
    ;;
  new-session)
    session_arg="${2#-s}"
    server=1
    if (( ${sessions[(Ie)$session_arg]} == 0 )); then
      sessions+=("$session_arg")
    fi
    save_state
    ;;
  kill-session)
    target="${1#-t}"
    sessions=(${sessions:#$target})
    if (( ${#sessions} > 0 )); then
      server=1
    else
      server=0
    fi
    save_state
    ;;
  move-window|rename-window|select-window|split-window|new-window|select-layout|clear-history|send-keys)
    ;;
  *)
    print -r -- "unexpected fake tmux command: $command_name" >&2
    exit 1
    ;;
esac
"#
}
