use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use retmux::restore::{resolve_backup_name, restore_from_path_with_adapter};
use retmux::tmux::TmuxAdapter;

const FIXTURES_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/legacy");

#[test]
fn restores_latest_backup_when_name_missing() {
    let sandbox = RestoreSandbox::new("latest-fallback");
    let backup_root = sandbox.backup_root();

    copy_fixture_backup(
        &Path::new(FIXTURES_ROOT)
            .join("default_socket")
            .join("backup_20240101_120000"),
        &backup_root,
    );
    write_backup(
        &backup_root,
        "backup_20240101_130000",
        latest_snapshot_json(),
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
    assert_contains(&log, "new-session -d -sretmux_dummy_");
    assert_contains(&log, "kill-session -tretmux_dummy_");
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
        conflict_snapshot_json(),
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
    fs::write(backup_dir.join("backup_bad.json"), "{\"tid\":true}\n")
        .expect("should write malformed snapshot");

    let adapter = sandbox.adapter_owned();
    let error = restore_from_path_with_adapter(&backup_root, &adapter, "backup_bad")
        .expect_err("malformed snapshot must fail");
    let message = error.to_string();

    assert!(
        message.contains("failed to load snapshot")
            && message.contains("missing required legacy marker __class__"),
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
        latest_snapshot_json(),
        &[("alpha:3.0", "session alpha main pane\n")],
    );

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
        !log.contains("new-session -d -sretmux_dummy_"),
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
fn named_restore_resolution_trims_and_rejects_invalid_names() {
    let sandbox = RestoreSandbox::new("normalized-restore-name");
    let backup_root = sandbox.backup_root();

    write_backup(
        &backup_root,
        "backup_trimmed",
        latest_snapshot_json(),
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

fn copy_fixture_backup(source_backup_dir: &Path, backup_root: &Path) {
    let backup_name = source_backup_dir
        .file_name()
        .expect("fixture backup dir should have a name");
    let destination = backup_root.join(backup_name);
    fs::create_dir_all(&destination).expect("should create copied fixture destination");

    for entry in fs::read_dir(source_backup_dir).expect("should list fixture backup directory") {
        let entry = entry.expect("fixture backup entry should be readable");
        let target = destination.join(entry.file_name());
        fs::copy(entry.path(), target).expect("fixture file should copy");
    }
}

fn write_backup(
    backup_root: &Path,
    backup_name: &str,
    snapshot_json: &str,
    pane_files: &[(&str, &str)],
) {
    let backup_dir = backup_root.join(backup_name);
    fs::create_dir_all(&backup_dir).expect("should create backup directory");
    fs::write(
        backup_dir.join(format!("{backup_name}.json")),
        snapshot_json,
    )
    .expect("should write snapshot json");

    for (pane_id, content) in pane_files {
        fs::write(backup_dir.join(pane_id), content).expect("should write pane content file");
    }
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

fn latest_snapshot_json() -> &'static str {
    r#"{
  "__class__": "Tmux",
  "__module__": "tmuxbk.tmux_obj",
  "create_time": "2024-01-01 13:00:00",
  "sessions": [
    {
      "__class__": "Session",
      "__module__": "tmuxbk.tmux_obj",
      "attached": false,
      "name": "alpha",
      "size": [140, 45],
      "windows": [
        {
          "__class__": "Window",
          "__module__": "tmuxbk.tmux_obj",
          "active": false,
          "layout": "1900,140x45,0,0,0",
          "name": "shell",
          "panes": [
            {
              "__class__": "Pane",
              "__module__": "tmuxbk.tmux_obj",
              "active": true,
              "cont_file": "",
              "pane_id": 0,
              "path": "/tmp/alpha/shell",
              "sess_name": "alpha",
              "size": [140, 45],
              "win_id": 1
            }
          ],
          "sess_name": "alpha",
          "win_id": 1
        },
        {
          "__class__": "Window",
          "__module__": "tmuxbk.tmux_obj",
          "active": true,
          "layout": "a1b2,140x45,0,0{70x45,0,0,0,69x45,71,0,1}",
          "name": "editor",
          "panes": [
            {
              "__class__": "Pane",
              "__module__": "tmuxbk.tmux_obj",
              "active": true,
              "cont_file": "",
              "pane_id": 0,
              "path": "/tmp/alpha/main",
              "sess_name": "alpha",
              "size": [70, 45],
              "win_id": 3
            },
            {
              "__class__": "Pane",
              "__module__": "tmuxbk.tmux_obj",
              "active": false,
              "cont_file": "",
              "pane_id": 1,
              "path": "/tmp/alpha/side",
              "sess_name": "alpha",
              "size": [69, 45],
              "win_id": 3
            }
          ],
          "sess_name": "alpha",
          "win_id": 3
        }
      ],
      "size": [140, 45]
    }
  ],
  "tid": "backup_20240101_130000"
}"#
}

fn conflict_snapshot_json() -> &'static str {
    r#"{
  "__class__": "Tmux",
  "__module__": "tmuxbk.tmux_obj",
  "create_time": "2024-01-02 12:00:00",
  "sessions": [
    {
      "__class__": "Session",
      "__module__": "tmuxbk.tmux_obj",
      "attached": false,
      "name": "existing",
      "size": [80, 24],
      "windows": [
        {
          "__class__": "Window",
          "__module__": "tmuxbk.tmux_obj",
          "active": true,
          "layout": "cafe,80x24,0,0,0",
          "name": "existing-window",
          "panes": [
            {
              "__class__": "Pane",
              "__module__": "tmuxbk.tmux_obj",
              "active": true,
              "cont_file": "",
              "pane_id": 0,
              "path": "/tmp/existing",
              "sess_name": "existing",
              "size": [80, 24],
              "win_id": 1
            }
          ],
          "sess_name": "existing",
          "win_id": 1
        }
      ]
    },
    {
      "__class__": "Session",
      "__module__": "tmuxbk.tmux_obj",
      "attached": false,
      "name": "fresh",
      "size": [90, 28],
      "windows": [
        {
          "__class__": "Window",
          "__module__": "tmuxbk.tmux_obj",
          "active": true,
          "layout": "dead,90x28,0,0,0",
          "name": "fresh-window",
          "panes": [
            {
              "__class__": "Pane",
              "__module__": "tmuxbk.tmux_obj",
              "active": true,
              "cont_file": "",
              "pane_id": 0,
              "path": "/tmp/fresh",
              "sess_name": "fresh",
              "size": [90, 28],
              "win_id": 2
            }
          ],
          "sess_name": "fresh",
          "win_id": 2
        }
      ]
    }
  ],
  "tid": "backup_20240102_120000"
}"#
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
            "retmux-restore-integration-{label}-{}-{unique}",
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
