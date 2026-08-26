mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::actions::restore::restore_from_path_with_adapter;
use remux::config::{AppConfig, ConfigPaths};
use remux::model::{Pane, Process};
use remux::storage::{self, LoadedSnapshot};

use crate::support::{single_window_restore_fake, single_window_tmux};

const FAKE_TMUX_SCRIPT: &str = include_str!("support/fake_tmux_backup.sh");
const BACKUP_NAME: &str = "backup_20240101_120000";

#[test]
fn live_pane_process_is_captured_as_the_tree_root() {
    let env = RemuxHome::new("live-pane");
    env.write_config();
    env.install_fake_tmux();

    let output = env.run_backup(BACKUP_NAME, env.live_pane_pid);
    assert_success(&output, "named backup should succeed");

    let snapshot = env.read_snapshot(BACKUP_NAME);
    let command_tree = first_pane(&snapshot)
        .command_tree
        .as_ref()
        .expect("a live pane process should be captured as the command tree root");
    assert_eq!(command_tree.pid, env.live_pane_pid);
}

#[test]
fn vanished_pane_process_is_not_invented() {
    let env = RemuxHome::new("vanished-pane");
    env.write_config();
    env.install_fake_tmux();

    let output = env.run_backup(BACKUP_NAME, u32::MAX);
    assert_success(
        &output,
        "backup should succeed when the pane process is gone",
    );
    assert!(
        first_pane(&env.read_snapshot(BACKUP_NAME))
            .command_tree
            .is_none(),
        "a vanished pane process must not be invented"
    );
}

#[test]
fn written_command_tree_survives_snapshot_round_trip() {
    let scratch = ScratchDir::new("round-trip");
    let backup_dir = scratch.path.join(BACKUP_NAME);
    let (mut tmux, pane_contents) =
        single_window_tmux(BACKUP_NAME, "work", "2024-01-01 12:00:00", &["/tmp/work"]);
    tmux.sessions[0].windows[0].panes[0].command_tree = Some(shell_with_foreground_editor());

    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("snapshot directory should be written");
    let loaded =
        storage::read_snapshot_dir(&backup_dir).expect("written snapshot should be readable");
    let command_tree = first_pane(&loaded)
        .command_tree
        .as_ref()
        .expect("the command tree should survive the snapshot round trip");

    assert!(
        command_tree
            .children
            .iter()
            .any(|child| child.foreground && child.name == "editor"),
        "expected a foreground editor child, got {command_tree:?}"
    );
}

#[test]
fn old_snapshot_without_command_tree_still_loads() {
    let loaded = storage::read_snapshot_dir(&support::schema_1_0_snapshot_dir())
        .expect("frozen schema 1.0 snapshot should still load");
    assert!(
        first_pane(&loaded).command_tree.is_none(),
        "an old snapshot without a command tree must still load as no tree"
    );
}

#[test]
fn restore_does_not_start_a_recorded_process() {
    let scratch = ScratchDir::new("restore");
    let backup_dir = scratch.path.join(BACKUP_NAME);
    let (mut tmux, pane_contents) =
        single_window_tmux(BACKUP_NAME, "work", "2024-01-01 12:00:00", &["/tmp/work"]);
    tmux.sessions[0].windows[0].panes[0].command_tree = Some(Process {
        name: "evil-binary".to_string(),
        argv: vec!["evil-binary".to_string(), "--pwn".to_string()],
        pid: 4242,
        foreground: true,
        children: Vec::new(),
    });
    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("restore fixture snapshot should be written");

    let fake = single_window_restore_fake(&backup_dir);
    restore_from_path_with_adapter(&scratch.path, &fake, BACKUP_NAME)
        .expect("restore should succeed with fake tmux");
    let restore_trace = format!("{:?}", fake.recorded_commands());
    assert!(
        !restore_trace.contains("evil-binary"),
        "restore must not start the recorded command_tree process, commands were: {restore_trace}"
    );
}

fn shell_with_foreground_editor() -> Process {
    Process {
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
                name: "editor".to_string(),
                argv: vec!["editor".to_string(), "/tmp/notes.md".to_string()],
                pid: 4242,
                foreground: true,
                children: Vec::new(),
            },
        ],
    }
}

fn first_pane(snapshot: &LoadedSnapshot) -> &Pane {
    &snapshot.tmux.sessions[0].windows[0].panes[0]
}

fn assert_success(output: &Output, context: &str) {
    assert!(
        output.status.success(),
        "{context}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct RemuxHome {
    root: PathBuf,
    home: PathBuf,
    binary_directory: PathBuf,
    live_pane_pid: u32,
}

impl RemuxHome {
    fn new(label: &str) -> Self {
        let root = unique_temp_dir(&format!("command-tree-{label}"));
        let home = root.join("home");
        let binary_directory = root.join("bin");
        fs::create_dir_all(&home).expect("should create fake HOME");
        fs::create_dir_all(&binary_directory).expect("should create fake bin dir");
        Self {
            root,
            home,
            binary_directory,
            live_pane_pid: std::process::id() as u32,
        }
    }

    fn config_paths(&self) -> ConfigPaths {
        ConfigPaths::from_home(&self.home)
    }

    fn write_config(&self) {
        let paths = self.config_paths();
        fs::create_dir_all(&paths.user_path).expect("should create ~/.remux");
        fs::write(
            &paths.config_file,
            "[logging]\nfile = \"info\"\nconsole = \"off\"\n\n[capture]\nwith_escape = true\n",
        )
        .expect("should write test config");
    }

    fn install_fake_tmux(&self) {
        let script_path = self.binary_directory.join("tmux");
        fs::write(&script_path, FAKE_TMUX_SCRIPT).expect("should write fake tmux script");
        let mut permissions = fs::metadata(&script_path)
            .expect("fake tmux script should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .expect("should make fake tmux script executable");
    }

    fn run_backup(&self, backup_name: &str, pane_pid: u32) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_remux"));
        command.args(["backup", backup_name]);
        command.env("HOME", &self.home);
        command.env("PATH", &self.binary_directory);
        command.env("TMUX_TMPDIR", &self.root);
        command.env("REMUX_FAKE_PANE_PID", pane_pid.to_string());
        command
            .output()
            .expect("binary invocation should complete successfully")
    }

    fn read_snapshot(&self, backup_name: &str) -> LoadedSnapshot {
        let backup_dir = self
            .config_paths()
            .backup_root(&AppConfig::default())
            .join(backup_name);
        storage::read_snapshot_dir(&backup_dir).expect("named backup snapshot should be readable")
    }
}

impl Drop for RemuxHome {
    fn drop(&mut self) {
        remove_dir_if_exists(&self.root);
    }
}

struct ScratchDir {
    path: PathBuf,
}

impl ScratchDir {
    fn new(label: &str) -> Self {
        Self {
            path: unique_temp_dir(&format!("command-tree-{label}")),
        }
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        remove_dir_if_exists(&self.path);
    }
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after UNIX_EPOCH")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("remux-{label}-{}-{unique}", std::process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).expect("stale temp dir should be removable");
    }
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn remove_dir_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_dir_all(path);
    }
}
