use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use remux::config::{AppState, ConfigPaths, ExecutionOptions};
use remux::model::{Pane, Session, Size, Tmux, Window};
use remux::storage;

mod support;

#[test]
fn interactive_list_without_arg_shows_details_until_quit() {
    let env = InteractiveEnv::new("interactive-list");
    env.write_config();
    env.write_model_backup(
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );

    let output = env.run_binary_with_stdin(&["-l"], "1\nq\n");
    assert_success(&output, "interactive list should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("remux> Please give backup No. (press q to exit):"),
        "expected interactive selection prompt, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("Details of backup:backup_20240101_120000"),
        "expected selected backup details, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("Backup─┬─[backup_20240101_120000] (1 sessions):"),
        "expected python-style backup tree output, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("─Session─┬─[work] (1 windows):"),
        "expected python-style session tree output, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("─Pane (0) /tmp/work"),
        "expected pane detail output, stdout was: {stdout}"
    );
}

#[test]
fn interactive_list_orders_backups_by_backup_id_desc_like_python() {
    let env = InteractiveEnv::new("interactive-list-order");
    env.write_config();
    env.write_model_backup(
        None,
        "backup_20240103_120000",
        "latest-by-name",
        "2024-01-03 12:00:00",
        &["/tmp/name-latest"],
    );
    env.write_model_backup(
        None,
        "backup_20240101_120000",
        "oldest-by-name",
        "2024-01-01 12:00:00",
        &["/tmp/name-oldest"],
    );
    env.write_model_backup(
        None,
        "backup_20240102_120000",
        "middle-by-name",
        "2024-01-02 12:00:00",
        &["/tmp/name-middle"],
    );

    let output = env.run_binary_with_stdin(&["-l"], "1\nq\n");
    assert_success(&output, "interactive list should order backups like python");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Details of backup:backup_20240103_120000"),
        "expected list index 1 to resolve to lexicographically latest backup id, stdout was: {stdout}"
    );
}

#[test]
fn interactive_list_without_input_returns_summary_successfully() {
    let env = InteractiveEnv::new("interactive-list-eof");
    env.write_config();
    env.write_model_backup(
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );

    let output = env.run_binary_with_stdin(&["-l"], "");
    assert_success(
        &output,
        "interactive list should allow EOF after printing summary",
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("backup_20240101_120000"),
        "expected backup summary to be printed, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("remux> Please give backup No. (press q to exit):"),
        "expected prompt to still be shown before EOF exit, stdout was: {stdout}"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.trim().is_empty(),
        "expected no stderr when list exits on EOF, stderr was: {stderr}"
    );
}

#[test]
fn interactive_delete_without_arg_confirms_before_deleting() {
    let env = InteractiveEnv::new("interactive-delete");
    env.write_config();
    let backup_dir = env.write_model_backup(
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );

    let output = env.run_binary_with_stdin(&["-d"], "1\nyes\n");
    assert_success(&output, "interactive delete should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Delete backup backup_20240101_120000? [yes|no]"),
        "expected delete confirmation prompt, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("Backup backup_20240101_120000 was deleted"),
        "expected delete success message, stdout was: {stdout}"
    );
    assert!(
        !backup_dir.exists(),
        "backup directory should be removed by interactive delete"
    );
}

#[test]
fn interactive_restore_accepts_scripted_input() {
    let env = InteractiveEnv::new("interactive-restore");
    env.write_config();
    env.install_fake_tmux();
    env.write_restore_backup(None, "backup_20240101_120000");

    let output = env.run_binary_with_stdin(&["-ri"], "1\nyes\n");
    assert_success(&output, "interactive restore should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("restore backup_20240101_120000? [yes|no]"),
        "expected restore confirmation prompt, stdout was: {stdout}"
    );
    assert!(
        stdout.contains("Backup backup_20240101_120000 was restored"),
        "expected restore success message, stdout was: {stdout}"
    );

    let log = env.read_fake_log();
    assert_contains(
        &log,
        "list-sessions -F#S:=:(#{window_width},#{window_height}):=:#{session_attached}",
    );
    assert_contains(&log, "new-session -d -sremux_dummy_");
    assert_contains(&log, "new-session -d -swork -x120 -y40");
    assert_contains(&log, "rename-window -twork:1 editor");
    assert_contains(&log, "send-keys -twork:1.0 builtin cd \"/tmp/work\"");
}

#[test]
fn invalid_input_and_eof_are_reported() {
    let invalid_env = InteractiveEnv::new("invalid-selection");
    invalid_env.write_config();
    let preserved_backup = invalid_env.write_model_backup(
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );

    let invalid_output =
        invalid_env.run_binary_with_stdin(&["-d"], "\nhello\n9\n1\nmaybe\nno\nq\n");
    assert_success(
        &invalid_output,
        "interactive delete should recover from invalid input and let the user quit",
    );
    let invalid_stdout = String::from_utf8_lossy(&invalid_output.stdout);
    assert!(
        invalid_stdout.contains("Invalid index: (empty)"),
        "expected empty index error, stdout was: {invalid_stdout}"
    );
    assert!(
        invalid_stdout.contains("Invalid index: hello"),
        "expected non-numeric index error, stdout was: {invalid_stdout}"
    );
    assert!(
        invalid_stdout.contains("Invalid index: 9"),
        "expected out-of-range index error, stdout was: {invalid_stdout}"
    );
    assert!(
        invalid_stdout.contains("Invalid confirmation: maybe"),
        "expected invalid confirmation error, stdout was: {invalid_stdout}"
    );
    assert!(
        preserved_backup.exists(),
        "backup should still exist after invalid interactive delete inputs"
    );

    let eof_env = InteractiveEnv::new("eof-selection");
    eof_env.write_config();
    let eof_backup = eof_env.write_model_backup(
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/work"],
    );

    let eof_output = eof_env.run_binary_with_stdin(&["-d"], "");
    assert!(
        !eof_output.status.success(),
        "EOF should return a nonzero exit for interactive delete"
    );
    let eof_stderr = String::from_utf8_lossy(&eof_output.stderr);
    assert!(
        eof_stderr.contains("end of input while reading backup selection"),
        "expected deterministic EOF error, stderr was: {eof_stderr}"
    );
    assert!(
        eof_backup.exists(),
        "EOF should not delete or mutate existing backups"
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

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected log to contain {needle:?}, got:\n{haystack}"
    );
}

struct InteractiveEnv {
    root: PathBuf,
    home: PathBuf,
    bin_dir: PathBuf,
    fake_log: PathBuf,
    fake_state: PathBuf,
}

impl InteractiveEnv {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "remux-interactive-flows-{label}-{}-{unique}",
            std::process::id()
        ));

        if root.exists() {
            fs::remove_dir_all(&root).expect("should clear stale interactive test root");
        }
        fs::create_dir_all(&root).expect("should create interactive test root");

        let home = root.join("home");
        let bin_dir = root.join("bin");
        let fake_log = root.join("fake-tmux.log");
        let fake_state = root.join("fake-tmux.state");
        fs::create_dir_all(&home).expect("should create fake HOME");
        fs::create_dir_all(&bin_dir).expect("should create fake bin dir");

        Self {
            root,
            home,
            bin_dir,
            fake_log,
            fake_state,
        }
    }

    fn write_config(&self) {
        let paths = self.config_paths();
        fs::create_dir_all(&paths.user_path).expect("should create ~/.remux");
        fs::write(
            &paths.config_file,
            "[logging]\nfile = \"info\"\nconsole = \"info\"\n\n[capture]\nwith_escape = true\n",
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

        fs::write(&self.fake_log, "").expect("should create fake tmux log");
        fs::write(&self.fake_state, "server=0\nsessions=\n")
            .expect("should create fake tmux state");
    }

    fn write_model_backup(
        &self,
        socket_name: Option<&str>,
        backup_id: &str,
        session_name: &str,
        create_time: &str,
        pane_paths: &[&str],
    ) -> PathBuf {
        let mut config = AppState::load_from_home(&self.home)
            .expect("runtime config should bootstrap temp HOME");
        config.set_execution_options(ExecutionOptions::with_socket_name(socket_name));

        let backup_dir = config.active_backup_path().join(backup_id);
        fs::create_dir_all(&backup_dir).expect("backup directory should be created");

        let (tmux, pane_contents) =
            support::single_window_tmux(backup_id, session_name, create_time, pane_paths);
        storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
            .expect("snapshot directory should be written");

        backup_dir
    }

    fn write_restore_backup(&self, socket_name: Option<&str>, backup_id: &str) -> PathBuf {
        let mut config = AppState::load_from_home(&self.home)
            .expect("runtime config should bootstrap temp HOME");
        config.set_execution_options(ExecutionOptions::with_socket_name(socket_name));

        let backup_dir = config.active_backup_path().join(backup_id);
        fs::create_dir_all(&backup_dir).expect("restore backup directory should be created");

        let mut tmux = Tmux::new(backup_id);
        tmux.create_time = "2024-01-01 12:00:00".to_string();
        let mut session = Session::new("work");
        session.size = Size::new(120, 40);

        let mut window = Window::new("work", 1);
        window.name = "editor".to_string();
        window.active = true;
        window.layout = "1900,120x40,0,0,0".to_string();

        let mut pane = Pane::new("work", 1, 0);
        pane.active = true;
        pane.size = Size::new(120, 40);
        pane.path = "/tmp/work".to_string();
        window.panes.push(pane);
        session.windows.push(window);
        tmux.sessions.push(session);

        let mut pane_contents = std::collections::BTreeMap::new();
        pane_contents.insert("work:1.0".to_string(), b"restored pane\n".to_vec());
        storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
            .expect("restore snapshot should be written");
        backup_dir
    }

    fn run_binary_with_stdin(&self, args: &[&str], stdin_payload: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_remux"));
        command
            .args(args)
            .env("HOME", &self.home)
            .env("PATH", &self.bin_dir)
            .env("REMUX_FAKE_LOG", &self.fake_log)
            .env("REMUX_FAKE_STATE", &self.fake_state)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .expect("interactive binary invocation should spawn successfully");
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(stdin_payload.as_bytes())
                .expect("should write scripted stdin");
        }

        child
            .wait_with_output()
            .expect("interactive binary invocation should complete")
    }

    fn read_fake_log(&self) -> String {
        if self.fake_log.exists() {
            fs::read_to_string(&self.fake_log).expect("fake tmux log should be readable")
        } else {
            String::new()
        }
    }

    fn config_paths(&self) -> ConfigPaths {
        ConfigPaths::from_home(&self.home)
    }
}

impl Drop for InteractiveEnv {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

const FAKE_TMUX_SCRIPT: &str = r#"#!/bin/zsh
set -eu

log_path="${REMUX_FAKE_LOG:-}"
state_path="${REMUX_FAKE_STATE:-}"

if [[ -z "$state_path" ]]; then
  print -r -- 'REMUX_FAKE_STATE is required' >&2
  exit 1
fi

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
  if [[ -n "$log_path" ]]; then
    print -r -- "$*" >> "$log_path"
  fi
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
"#;
