use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::config::{ConfigPaths, socket_dir_name};

#[test]
#[ignore = "requires tmux on the supported Linux baseline"]
fn backup_list_and_delete_work_against_real_tmux() {
    let env = LiveTmuxEnv::new("backup-list-delete");
    env.start_session("work", "editor");

    let backup_name = "backup_20240101_120000";
    let backup_output = env.run_binary(&["-L", env.socket_name(), "-b", backup_name]);
    assert_success(&backup_output, "live tmux backup should succeed");

    let backup_dir = env.backup_dir(backup_name);
    let snapshot_path = backup_dir.join(format!("{backup_name}.json"));
    let backup_stdout = String::from_utf8_lossy(&backup_output.stdout);
    assert!(
        snapshot_path.is_file(),
        "expected snapshot at {}",
        snapshot_path.display()
    );
    assert!(
        backup_stdout.contains(&format!(
            "Backup of sessions was saved under {}",
            backup_dir.display()
        )),
        "expected backup success message, stdout was: {backup_stdout}"
    );

    let list_output = env.run_binary(&["-L", env.socket_name(), "-l", backup_name]);
    assert_success(&list_output, "listing a live-created backup should succeed");

    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        list_stdout.contains(&format!("Details of backup:{backup_name}")),
        "expected detail header, stdout was: {list_stdout}"
    );
    assert!(
        list_stdout.contains("─Session─┬─[work] (1 windows):"),
        "expected session detail, stdout was: {list_stdout}"
    );
    assert!(
        list_stdout.contains("[editor] (1 panes):"),
        "expected window detail, stdout was: {list_stdout}"
    );

    let delete_output = env.run_binary(&["-L", env.socket_name(), "-d", backup_name]);
    assert_success(
        &delete_output,
        "deleting a live-created backup should succeed",
    );
    assert!(
        !backup_dir.exists(),
        "expected backup directory {} to be removed",
        backup_dir.display()
    );
}

#[test]
#[ignore = "requires tmux on the supported Linux baseline"]
fn restore_recreates_session_against_real_tmux() {
    let env = LiveTmuxEnv::new("restore-session");
    env.start_session("restoreme", "editor");

    let backup_name = "backup_20240101_130000";
    let backup_output = env.run_binary(&["-L", env.socket_name(), "-b", backup_name]);
    assert_success(&backup_output, "backup before live restore should succeed");

    env.kill_server();

    let restore_output = env.run_binary(&["-L", env.socket_name(), "-r", backup_name]);
    assert_success(&restore_output, "live tmux restore should succeed");

    let sessions = env.tmux_stdout(&["list-sessions", "-F", "#S"]);
    assert!(
        sessions.lines().any(|line| line == "restoreme"),
        "expected restored session in tmux, stdout was: {sessions}"
    );

    let windows = env.tmux_stdout(&["list-windows", "-trestoreme", "-F", "#W"]);
    assert!(
        windows.lines().any(|line| line == "editor"),
        "expected restored window name in tmux, stdout was: {windows}"
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

#[derive(Debug)]
struct LiveTmuxEnv {
    root: PathBuf,
    home: PathBuf,
    workspace: PathBuf,
    socket_name: String,
}

impl LiveTmuxEnv {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "remux-live-tmux-{label}-{}-{unique}",
            std::process::id()
        ));

        if root.exists() {
            fs::remove_dir_all(&root).expect("should clear stale live tmux test root");
        }

        let home = root.join("home");
        let workspace = root.join("workspace");
        fs::create_dir_all(&home).expect("should create fake HOME");
        fs::create_dir_all(&workspace).expect("should create live tmux workspace");

        Self {
            root,
            home,
            workspace,
            socket_name: format!("remux-live-{}-{unique}", std::process::id()),
        }
    }

    fn socket_name(&self) -> &str {
        &self.socket_name
    }

    fn config_paths(&self) -> ConfigPaths {
        ConfigPaths::from_home(&self.home)
    }

    fn backup_dir(&self, backup_name: &str) -> PathBuf {
        self.config_paths()
            .backup_socket_root
            .join(socket_dir_name(Some(self.socket_name())).expect("socket name should sanitize"))
            .join(backup_name)
    }

    fn run_binary(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_remux"))
            .env("HOME", &self.home)
            .args(args)
            .output()
            .expect("remux binary invocation should succeed")
    }

    fn start_session(&self, session_name: &str, window_name: &str) {
        let workspace = self.workspace.to_string_lossy().into_owned();
        let output = self.run_tmux(&[
            "new-session",
            "-d",
            "-s",
            session_name,
            "-n",
            window_name,
            "-c",
            &workspace,
            "-x",
            "100",
            "-y",
            "30",
        ]);
        assert_success(&output, "starting live tmux session should succeed");
    }

    fn tmux_stdout(&self, args: &[&str]) -> String {
        let output = self.run_tmux(args);
        assert_success(&output, "tmux command should succeed");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn run_tmux(&self, args: &[&str]) -> Output {
        Command::new("tmux")
            .env("HOME", &self.home)
            .arg("-L")
            .arg(&self.socket_name)
            .args(args)
            .output()
            .expect("tmux command should spawn")
    }

    fn kill_server(&self) {
        let _ = self.run_tmux(&["kill-server"]);
    }
}

impl Drop for LiveTmuxEnv {
    fn drop(&mut self) {
        self.kill_server();
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
