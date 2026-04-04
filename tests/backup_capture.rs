use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::config::{AppConfig, ConfigPaths, socket_dir_name};
use remux::storage;

#[test]
fn creates_snapshot_tree() {
    let env = TestEnv::new("creates-tree");
    env.write_config(true);
    env.install_fake_tmux();

    let output = env.run_binary(&["-L", "sock/name", "backup", "backup_20240101_120000"]);
    assert_success(&output, "named-socket backup should succeed");

    let backup_root = env
        .config_paths()
        .backup_socket_root(&AppConfig::default())
        .join(socket_dir_name(Some("sock/name")).unwrap());
    let backup_dir = backup_root.join("backup_20240101_120000");
    let summary_path = backup_dir.join("summary.json");
    let manifest_path = backup_dir.join("manifest.json");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        summary_path.is_file(),
        "expected snapshot summary at {}",
        summary_path.display()
    );
    assert!(
        manifest_path.is_file(),
        "expected snapshot manifest at {}",
        manifest_path.display()
    );
    assert!(
        backup_dir.join("panes").join("work:1.0.txt").is_file(),
        "expected pane file work:1.0.txt"
    );
    assert!(
        backup_dir.join("panes").join("work:1.1.txt").is_file(),
        "expected pane file work:1.1.txt"
    );
    assert!(
        stdout.contains(&format!(
            "Backup of sessions was saved under {}",
            backup_dir.display()
        )),
        "expected backup success message to mention backup path, stdout was: {stdout}"
    );

    let snapshot = storage::read_snapshot_dir(&backup_dir)
        .expect("generated snapshot should decode as Rust snapshot directory");
    let tmux = &snapshot.tmux;
    assert_eq!(tmux.tid, "backup_20240101_120000");
    assert_create_time_shape(&tmux.create_time);
    assert_eq!(tmux.sessions.len(), 1);

    let session = &tmux.sessions[0];
    assert_eq!(session.name, "work");
    assert_eq!(session.size.as_tuple(), Some((120, 40)));
    assert!(!session.attached);
    assert_eq!(session.windows.len(), 1);

    let window = &session.windows[0];
    assert_eq!(window.win_id, 1);
    assert_eq!(window.name, "editor");
    assert!(window.active);
    assert_eq!(window.layout, "1900,120x40,0,0,0");
    assert_eq!(window.panes.len(), 2);

    let pane_ids = window
        .panes
        .iter()
        .map(|pane| pane.idstr())
        .collect::<Vec<_>>();
    assert_eq!(pane_ids, vec!["work:1.0", "work:1.1"]);

    assert_eq!(
        fs::read(backup_dir.join("panes").join("work:1.0.txt"))
            .expect("pane content should be readable"),
        b"pane0 with escape \x1b[31mred\x1b[0m\n"
    );
    assert_eq!(
        fs::read(backup_dir.join("panes").join("work:1.1.txt"))
            .expect("pane content should be readable"),
        b"pane1 with escape \x1b[32mgreen\x1b[0m\n"
    );

    let log = env.read_fake_log();
    assert!(
        log.lines()
            .any(|line| line.contains("capture-pane -ep -S-100000 -twork:1.0")),
        "expected capture-pane to use -ep, log was:\n{log}"
    );
}

#[test]
fn duplicate_backup_id_fails() {
    let env = TestEnv::new("duplicate-id");
    env.write_config(true);

    let paths = env.config_paths();
    let backup_dir = paths
        .backup_root(&AppConfig::default())
        .join("existing_backup");
    fs::create_dir_all(&backup_dir).expect("should create duplicate backup dir");
    let sentinel = backup_dir.join("summary.json");
    fs::write(&sentinel, "sentinel").expect("should write sentinel snapshot");

    let output = env.run_binary(&["backup", "existing_backup"]);
    assert!(
        !output.status.success(),
        "duplicate backup id should exit nonzero: {output:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already exists") && !stderr.contains("binary not found"),
        "expected duplicate-id error, got stderr: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&sentinel).expect("sentinel file should remain readable"),
        "sentinel"
    );
}

#[test]
fn invalid_backup_names_fail_before_tmux_probe() {
    let env = TestEnv::new("invalid-backup-names");
    env.write_config(true);

    for invalid_name in [
        "",
        "   ",
        "..",
        "nested/name",
        "nested\\name",
        "/tmp/backup",
    ] {
        let output = env.run_binary(&["backup", invalid_name]);
        assert!(
            !output.status.success(),
            "invalid backup name {invalid_name:?} should exit nonzero"
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid backup name") && !stderr.contains("binary not found"),
            "expected deterministic invalid-name failure for {invalid_name:?}, stderr was: {stderr}"
        );
    }

    assert!(
        env.read_fake_log().trim().is_empty(),
        "invalid backup names should fail before any tmux command is attempted"
    );
}

#[test]
fn trimmed_backup_name_is_normalized_for_create_and_lookup() {
    let env = TestEnv::new("trimmed-backup-name");
    env.write_config(true);
    env.install_fake_tmux();

    let create = env.run_binary(&["backup", "  backup_trimmed  "]);
    assert_success(&create, "trimmed backup name should create successfully");

    let backup_dir = env
        .config_paths()
        .backup_root(&AppConfig::default())
        .join("backup_trimmed");
    assert!(
        backup_dir.is_dir(),
        "expected normalized backup directory at {}",
        backup_dir.display()
    );
    assert!(
        !env.config_paths()
            .backup_root(&AppConfig::default())
            .join("  backup_trimmed  ")
            .exists(),
        "raw whitespace-padded backup directory should not be created"
    );

    let detail = env.run_binary(&["list", "backup_trimmed"]);
    assert_success(&detail, "lookup should succeed with normalized backup name");

    let detail_stdout = String::from_utf8_lossy(&detail.stdout);
    assert!(
        detail_stdout.contains("Backup: backup_trimmed"),
        "expected detail lookup to reuse normalized backup id, stdout was: {detail_stdout}"
    );
}

#[test]
fn default_backup_name_uses_timestamp_and_plain_capture_flag() {
    let env = TestEnv::new("timestamp-fallback");
    env.write_config(false);
    env.install_fake_tmux();

    let output = env.run_binary(&["backup"]);
    assert_success(&output, "unnamed backup should succeed");

    let backup_root = env.config_paths().backup_root(&AppConfig::default());
    let backup_ids = list_directory_names(&backup_root);
    assert_eq!(
        backup_ids.len(),
        1,
        "expected one generated backup directory"
    );

    let backup_id = &backup_ids[0];
    assert_backup_id_shape(backup_id);

    let backup_dir = backup_root.join(backup_id);
    let summary_path = backup_dir.join("summary.json");
    let manifest_path = backup_dir.join("manifest.json");
    assert!(
        summary_path.is_file(),
        "expected generated snapshot summary"
    );
    assert!(
        manifest_path.is_file(),
        "expected generated snapshot manifest"
    );
    assert_eq!(
        fs::read(backup_dir.join("panes").join("work:1.0.txt"))
            .expect("pane content should be readable"),
        b"pane0 plain\n"
    );

    let log = env.read_fake_log();
    assert!(
        log.lines()
            .any(|line| line.contains("capture-pane -p -S-100000 -twork:1.0")),
        "expected capture-pane to use -p, log was:\n{log}"
    );
    assert!(
        !log.lines().any(|line| line.contains("capture-pane -ep ")),
        "did not expect -ep when content.with.escape is false, log was:\n{log}"
    );
}

#[test]
fn no_server_exits_cleanly() {
    let env = TestEnv::new("no-server");
    env.write_config(true);
    env.install_fake_tmux();

    let output =
        env.run_binary_with_no_server(&["-L", "sock/name", "backup", "backup_20240101_120000"]);
    assert_success(&output, "no-server backup should be a clean no-op");

    let named_root = env
        .config_paths()
        .backup_socket_root(&AppConfig::default())
        .join(socket_dir_name(Some("sock/name")).unwrap());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !named_root.exists(),
        "no-server path should not create named-socket backup directories"
    );
    assert!(
        stdout.contains("No tmux session found, nothing to backup"),
        "expected no-server success message, stdout was: {stdout}"
    );

    let log = env.read_fake_log();
    assert!(
        log.lines()
            .any(|line| line.starts_with("-L sock/name list-sessions ")),
        "expected list-sessions probe before clean no-op, log was:\n{log}"
    );
    assert!(
        !log.lines().any(|line| line.contains("capture-pane")),
        "no-server path should not capture panes, log was:\n{log}"
    );
}

#[test]
fn legacy_socket_flag_is_rejected_before_any_tmux_call() {
    let env = TestEnv::new("legacy-socket-flag");
    env.write_config(true);
    env.install_fake_tmux();

    let output = env.run_binary(&["--socket", "sock/name", "backup", "backup_20240101_120000"]);
    assert!(
        !output.status.success(),
        "legacy --socket flag should exit nonzero: {output:?}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unexpected argument '--socket' found"),
        "expected clap to reject legacy --socket, stderr was: {stderr}"
    );
    assert!(
        env.read_fake_log().trim().is_empty(),
        "legacy --socket must fail before any tmux command is attempted"
    );
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_create_time_shape(value: &str) {
    let bytes = value.as_bytes();
    assert_eq!(bytes.len(), 19, "unexpected create_time shape: {value}");
    assert_eq!(bytes[4], b'-');
    assert_eq!(bytes[7], b'-');
    assert_eq!(bytes[10], b' ');
    assert_eq!(bytes[13], b':');
    assert_eq!(bytes[16], b':');
    assert!(
        bytes
            .iter()
            .enumerate()
            .all(|(idx, ch)| matches!(idx, 4 | 7 | 10 | 13 | 16) || ch.is_ascii_digit()),
        "unexpected create_time content: {value}"
    );
}

fn assert_backup_id_shape(value: &str) {
    let bytes = value.as_bytes();
    assert_eq!(bytes.len(), 15, "unexpected backup id shape: {value}");
    assert_eq!(bytes[8], b'_');
    assert!(
        bytes
            .iter()
            .enumerate()
            .all(|(idx, ch)| idx == 8 || ch.is_ascii_digit()),
        "unexpected backup id content: {value}"
    );
}

fn list_directory_names(path: &Path) -> Vec<String> {
    let mut names = fs::read_dir(path)
        .expect("directory should be readable")
        .map(|entry| {
            entry
                .expect("directory entry should be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

struct TestEnv {
    root: PathBuf,
    home: PathBuf,
    bin_dir: PathBuf,
    fake_log: PathBuf,
}

impl TestEnv {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "remux-backup-capture-{label}-{}-{unique}",
            std::process::id()
        ));

        if root.exists() {
            fs::remove_dir_all(&root).expect("should clear stale temp test root");
        }
        fs::create_dir_all(&root).expect("should create temp test root");

        let home = root.join("home");
        let bin_dir = root.join("bin");
        let fake_log = root.join("fake-tmux.log");
        fs::create_dir_all(&home).expect("should create fake HOME");
        fs::create_dir_all(&bin_dir).expect("should create fake bin dir");

        Self {
            root,
            home,
            bin_dir,
            fake_log,
        }
    }

    fn config_paths(&self) -> ConfigPaths {
        ConfigPaths::from_home(&self.home)
    }

    fn write_config(&self, content_with_escape: bool) {
        let paths = self.config_paths();
        fs::create_dir_all(&paths.user_path).expect("should create ~/.remux");
        fs::write(
            &paths.config_file,
            format!(
                "[logging]\nfile = \"info\"\nconsole = \"info\"\n\n[capture]\nwith_escape = {}\n",
                if content_with_escape { "true" } else { "false" }
            ),
        )
        .expect("should write test config");
    }

    fn install_fake_tmux(&self) {
        let script_path = self.bin_dir.join("tmux");
        fs::write(&script_path, FAKE_TMUX_SCRIPT).expect("should write fake tmux script");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake tmux script should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .expect("should make fake tmux script executable");
    }

    fn run_binary(&self, args: &[&str]) -> Output {
        self.run_binary_inner(args, false)
    }

    fn run_binary_with_no_server(&self, args: &[&str]) -> Output {
        self.run_binary_inner(args, true)
    }

    fn read_fake_log(&self) -> String {
        if self.fake_log.exists() {
            fs::read_to_string(&self.fake_log).expect("fake tmux log should be readable")
        } else {
            String::new()
        }
    }

    fn run_binary_inner(&self, args: &[&str], no_server: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_remux"));
        command.args(args);
        command.env("HOME", &self.home);
        command.env("PATH", &self.bin_dir);
        command.env("REMUX_FAKE_LOG", &self.fake_log);
        if no_server {
            command.env("REMUX_FAKE_NO_SERVER", "1");
        }
        command
            .output()
            .expect("binary invocation should complete successfully")
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

const FAKE_TMUX_SCRIPT: &str = r#"#!/bin/sh
set -eu

if [ -n "${REMUX_FAKE_LOG:-}" ]; then
  printf '%s\n' "$*" >> "$REMUX_FAKE_LOG"
fi

if [ "${1:-}" = "-L" ]; then
  shift 2
fi

case "${1:-}" in
  list-sessions)
    if [ "${REMUX_FAKE_NO_SERVER:-0}" = "1" ]; then
      exit 1
    fi
    printf 'work:=:(120,40):=:0\n'
    ;;
  list-windows)
    target=''
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -t*)
          target="${1#-t}"
          ;;
      esac
      shift
    done
    case "$target" in
      work)
        printf '1:=:editor:=:1:=:1900,120x40,0,0,0\n'
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  list-panes)
    target=''
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -t*)
          target="${1#-t}"
          ;;
      esac
      shift
    done
    case "$target" in
      work:1)
        printf '0:=:(120,20):=:/tmp/work:=:1\n1:=:(120,20):=:/tmp/logs:=:0\n'
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  capture-pane)
    target=''
    flag=''
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -ep|-p)
          flag="$1"
          ;;
        -t*)
          target="${1#-t}"
          ;;
      esac
      shift
    done
    case "$target" in
      work:1.0)
        if [ "$flag" = "-ep" ]; then
          printf 'pane0 with escape \033[31mred\033[0m\n'
        else
          printf 'pane0 plain\n'
        fi
        ;;
      work:1.1)
        if [ "$flag" = "-ep" ]; then
          printf 'pane1 with escape \033[32mgreen\033[0m\n'
        else
          printf 'pane1 plain\n'
        fi
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  *)
    exit 1
    ;;
esac
"#;
