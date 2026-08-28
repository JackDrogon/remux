use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

use std::collections::BTreeMap;

use remux::config::{AppState, ExecutionOptions};
use remux::model::{Pane, Process, Tmux, Window};
use remux::storage;

mod support;

#[test]
fn compact_removes_older_automatic_duplicate() {
    let temp_home = TempHome::new("remove-duplicate");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(output.stderr.is_empty(), "stderr was {:?}", output.stderr);
    assert!(!older.exists(), "older automatic backup should be removed");
    assert!(newer.exists(), "newer backup should remain");
}

#[test]
fn compact_keeps_both_when_root_pid_differs() {
    let temp_home = TempHome::new("different-pid");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18422, "zsh", &["-zsh"])),
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Latest backups 20240101_120500 and 20240101_120000 differ, nothing to compact\n"
    );
    assert!(
        older.exists(),
        "older backup should remain when pids differ"
    );
    assert!(
        newer.exists(),
        "newer backup should remain when pids differ"
    );
}

#[test]
fn compact_does_not_delete_a_named_previous_backup() {
    let temp_home = TempHome::new("named-previous");
    let named = write_backup(
        temp_home.path(),
        None,
        "before-refactor",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Previous backup before-refactor is not an automatic backup\n"
    );
    assert!(named.exists(), "named previous backup must not be deleted");
    assert!(newer.exists(), "newer backup should remain");
}

#[test]
fn compact_ignores_cwd_and_child_processes() {
    let temp_home = TempHome::new("ignore-cwd-children");
    let mut older_child = root(18421, "zsh", &["-zsh"]);
    older_child.children.push(root(20001, "vim", &["vim", "a"]));
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(older_child),
        1,
    );

    let mut newer_child = root(18421, "zsh", &["-zsh"]);
    newer_child.foreground = false;
    newer_child
        .children
        .push(root(20002, "cargo", &["cargo", "t"]));
    let newer_dir = write_backup_with_path(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        "/tmp/other",
        Some(newer_child),
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(!older.exists(), "cwd and children must not block compact");
    assert!(newer_dir.exists(), "newer backup should remain");
}

#[test]
fn compact_needs_two_backups() {
    let temp_home = TempHome::new("need-two");
    write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Need at least two backups to compact\n"
    );
}

#[test]
fn compact_does_not_delete_older_when_newer_pane_payload_is_unreadable() {
    let temp_home = TempHome::new("corrupt-newer-pane");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );
    fs::remove_file(newer.join("panes").join("work:1.0.txt"))
        .expect("newer pane file should be removable");

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        !output.status.success(),
        "unreadable newer pane payload should fail compact: {output:?}"
    );
    assert!(
        older.exists(),
        "older readable backup must not be deleted when the newer payload is broken"
    );
    assert!(newer.exists(), "newer backup directory should remain");
}

#[test]
fn compact_ignores_a_third_older_backup() {
    let temp_home = TempHome::new("third-backup");
    let oldest = write_backup(
        temp_home.path(),
        None,
        "20240101_115000",
        "other",
        Some(root(1, "zsh", &["-zsh"])),
        1,
    );
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );
    let newest = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        3,
    );
    fs::write(oldest.join("summary.json"), "{ not-json").expect("oldest snapshot should break");

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "a broken third backup must not block compact of the newest pair: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(oldest.exists(), "third backup must not be deleted");
    assert!(
        !older.exists(),
        "older of the newest pair should be removed"
    );
    assert!(newest.exists(), "newest backup should remain");
}

#[test]
fn compact_needs_two_backups_when_catalog_is_empty() {
    let temp_home = TempHome::new("empty-catalog");
    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "empty catalog compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Need at least two backups to compact\n"
    );
}

#[test]
fn compact_breaks_mtime_ties_by_backup_id_desc() {
    let temp_home = TempHome::new("mtime-tie");
    let older_id = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer_id = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(
        !older_id.exists(),
        "lower backup id should be treated as older"
    );
    assert!(newer_id.exists(), "higher backup id should be kept");
}

#[test]
fn compact_ignores_dot_prefixed_entries_even_if_metadata_fails() {
    let temp_home = TempHome::new("dangling-dot");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );
    std::os::unix::fs::symlink(
        "/no/such/remux-dot-target",
        newer.parent().expect("backup root").join(".dangling"),
    )
    .expect("dangling dot symlink should be created");

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "dangling dot entries must not fail compact: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(!older.exists(), "older automatic backup should be removed");
    assert!(newer.exists(), "newer backup should remain");
}

#[test]
fn compact_ignores_dot_prefixed_write_temp_directories() {
    let temp_home = TempHome::new("ignore-tmp");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let newer = write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );
    let tmp_dir = newer
        .parent()
        .expect("backup root")
        .join(".20240101_120600.tmp-1-2");
    fs::create_dir_all(&tmp_dir).expect("write temp dir should be created");
    fs::write(tmp_dir.join("summary.json"), "{ not-json").expect("temp summary should be junk");
    let tmp_mtime = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + 9);
    fs::File::open(&tmp_dir)
        .expect("temp dir should open")
        .set_modified(tmp_mtime)
        .expect("temp dir should look newest");

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "dot-prefixed temp dirs must not be selected: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(!older.exists(), "older automatic backup should be removed");
    assert!(newer.exists(), "newer backup should remain");
    assert!(tmp_dir.exists(), "write temp dir must be left untouched");
}

#[test]
fn compact_deletes_older_automatic_when_newer_is_named() {
    let temp_home = TempHome::new("named-newer");
    let automatic = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let named = write_backup(
        temp_home.path(),
        None,
        "before-refactor",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by before-refactor)\n"
    );
    assert!(
        !automatic.exists(),
        "older automatic duplicate should be removed"
    );
    assert!(named.exists(), "newer named backup should remain");
}

#[test]
fn compact_keeps_schema_1_0_and_1_1_apart() {
    let temp_home = TempHome::new("schema-mismatch");
    let newer = write_backup(temp_home.path(), None, "20240101_120500", "work", None, 2);
    let older = newer.parent().expect("backup root").join("20240101_120000");
    copy_dir(&support::schema_1_0_snapshot_dir(), &older);
    let older_mtime = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + 1);
    fs::File::open(&older)
        .expect("copied 1.0 dir should open")
        .set_modified(older_mtime)
        .expect("1.0 mtime should be older");

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "schema mismatch compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Latest backups 20240101_120500 and 20240101_120000 differ, nothing to compact\n"
    );
    assert!(older.exists(), "schema 1.0 backup should remain");
    assert!(newer.exists(), "schema 1.1 backup should remain");
}

#[test]
fn compact_is_isolated_to_the_active_socket() {
    let temp_home = TempHome::new("socket-isolation");
    let default_older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    write_backup(
        temp_home.path(),
        None,
        "20240101_120500",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        2,
    );
    let named_older = write_backup(
        temp_home.path(),
        Some("sockA"),
        "20240101_120000",
        "ops",
        Some(root(30001, "zsh", &["-zsh"])),
        1,
    );
    write_backup(
        temp_home.path(),
        Some("sockA"),
        "20240101_120500",
        "ops",
        Some(root(30001, "zsh", &["-zsh"])),
        2,
    );

    let output = run_binary(temp_home.path(), ["-L", "sockA", "compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert!(
        default_older.exists(),
        "default-root older backup must remain"
    );
    assert!(
        !named_older.exists(),
        "active socket older duplicate should be removed"
    );
}

#[test]
fn compact_removes_older_when_newer_adds_session() {
    let temp_home = TempHome::new("extra-session");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let (mut tmux, pane_contents) = work_tmux_with_root("20240101_120500", 18421);
    let (extra, extra_panes) = support::single_window_tmux(
        "20240101_120500",
        "other",
        "2024-01-01 12:00:00",
        &["/tmp/other"],
    );
    tmux.sessions.extend(extra.sessions);
    let mut pane_contents = pane_contents;
    pane_contents.extend(extra_panes);
    let newer = write_tmux_backup(
        temp_home.path(),
        "20240101_120500",
        &tmux,
        &pane_contents,
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Removed backup 20240101_120000 (covered by 20240101_120500)\n"
    );
    assert!(!older.exists(), "older subset backup should be removed");
    assert!(newer.exists(), "newer superset backup should remain");
}

#[test]
fn compact_removes_older_when_newer_adds_window() {
    let temp_home = TempHome::new("extra-window");
    let older = write_backup(
        temp_home.path(),
        None,
        "20240101_120000",
        "work",
        Some(root(18421, "zsh", &["-zsh"])),
        1,
    );
    let (mut tmux, mut pane_contents) = work_tmux_with_root("20240101_120500", 18421);
    let mut extra_window = Window::new("work", 2);
    extra_window.layout = "1901,120x40,0,0,1".to_string();
    let mut extra_pane = Pane::new("work", 2, 0);
    extra_pane.path = "/tmp/other".to_string();
    pane_contents.insert(
        extra_pane.pane_target().into_string(),
        b"extra window\n".to_vec(),
    );
    extra_window.panes.push(extra_pane);
    tmux.sessions[0].windows.push(extra_window);
    let newer = write_tmux_backup(
        temp_home.path(),
        "20240101_120500",
        &tmux,
        &pane_contents,
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert!(!older.exists(), "older subset backup should be removed");
    assert!(newer.exists(), "newer superset backup should remain");
}

#[test]
fn compact_keeps_both_when_newer_drops_session() {
    let temp_home = TempHome::new("dropped-session");
    let (mut older_tmux, older_panes) = work_tmux_with_root("20240101_120000", 18421);
    let (extra, extra_panes) = support::single_window_tmux(
        "20240101_120000",
        "other",
        "2024-01-01 12:00:00",
        &["/tmp/other"],
    );
    older_tmux.sessions.extend(extra.sessions);
    let mut older_panes = older_panes;
    older_panes.extend(extra_panes);
    let older = write_tmux_backup(
        temp_home.path(),
        "20240101_120000",
        &older_tmux,
        &older_panes,
        1,
    );
    let (newer_tmux, newer_panes) = work_tmux_with_root("20240101_120500", 18421);
    let newer = write_tmux_backup(
        temp_home.path(),
        "20240101_120500",
        &newer_tmux,
        &newer_panes,
        2,
    );

    let output = run_binary(temp_home.path(), ["compact"]);
    assert!(
        output.status.success(),
        "compact should succeed: {output:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "Latest backups 20240101_120500 and 20240101_120000 differ, nothing to compact\n"
    );
    assert!(
        older.exists(),
        "older backup with extra session should remain"
    );
    assert!(newer.exists(), "newer backup should remain");
}

fn work_tmux_with_root(backup_id: &str, pid: u32) -> (Tmux, BTreeMap<String, Vec<u8>>) {
    let (mut tmux, pane_contents) =
        support::single_window_tmux(backup_id, "work", "2024-01-01 12:00:00", &["/tmp/work"]);
    tmux.sessions[0].windows[0].panes[0].command_tree = Some(root(pid, "zsh", &["-zsh"]));
    (tmux, pane_contents)
}

fn write_tmux_backup(
    home_dir: &Path,
    backup_id: &str,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
    order: u64,
) -> PathBuf {
    let config =
        AppState::load_from_home(home_dir).expect("runtime config should bootstrap temp HOME");
    let backup_dir = config.active_backup_path().join(backup_id);
    storage::write_snapshot_dir(&backup_dir, tmux, pane_contents)
        .expect("snapshot directory should be written");
    let modified = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + order);
    fs::File::open(&backup_dir)
        .expect("backup directory should open")
        .set_modified(modified)
        .expect("backup directory mtime should be set");
    backup_dir
}

fn root(pid: u32, name: &str, argv: &[&str]) -> Process {
    Process {
        name: name.to_string(),
        argv: argv.iter().map(|value| (*value).to_string()).collect(),
        pid,
        foreground: true,
        children: Vec::new(),
    }
}

fn write_backup(
    home_dir: &Path,
    socket_name: Option<&str>,
    backup_id: &str,
    session_name: &str,
    command_root: Option<Process>,
    order: u64,
) -> PathBuf {
    write_backup_with_path(
        home_dir,
        socket_name,
        backup_id,
        session_name,
        "/tmp/work",
        command_root,
        order,
    )
}

fn write_backup_with_path(
    home_dir: &Path,
    socket_name: Option<&str>,
    backup_id: &str,
    session_name: &str,
    pane_path: &str,
    command_root: Option<Process>,
    order: u64,
) -> PathBuf {
    let mut config =
        AppState::load_from_home(home_dir).expect("runtime config should bootstrap temp HOME");
    config.set_execution_options(ExecutionOptions::with_socket_name(socket_name));

    let backup_dir = config.active_backup_path().join(backup_id);

    let (mut tmux, pane_contents) =
        support::single_window_tmux(backup_id, session_name, "2024-01-01 12:00:00", &[pane_path]);
    tmux.sessions[0].windows[0].panes[0].command_tree = command_root;
    storage::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("snapshot directory should be written");
    let modified = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + order);
    fs::File::open(&backup_dir)
        .expect("backup directory should open")
        .set_modified(modified)
        .expect("backup directory mtime should be set");
    backup_dir
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("destination directory should be created");
    for entry in fs::read_dir(from).expect("source directory should be readable") {
        let entry = entry.expect("source entry should be readable");
        let destination = to.join(entry.file_name());
        if entry
            .file_type()
            .expect("entry type should be readable")
            .is_dir()
        {
            copy_dir(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).expect("file should copy");
        }
    }
}

fn run_binary<const N: usize>(home_dir: &Path, args: [&str; N]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_remux"))
        .env("HOME", home_dir)
        .args(args)
        .output()
        .expect("remux binary invocation should succeed")
}

#[derive(Debug)]
struct TempHome {
    path: PathBuf,
}

impl TempHome {
    fn new(label: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-compact-{label}-{}-{unique}",
            std::process::id()
        ));
        if path.exists() {
            fs::remove_dir_all(&path).expect("should clear stale temp HOME");
        }
        fs::create_dir_all(&path).expect("should create temp HOME");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempHome {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
