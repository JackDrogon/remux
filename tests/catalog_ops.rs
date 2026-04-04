use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use remux::catalog;
use remux::config::{AppState, ExecutionOptions};
use remux::snapshot;

mod support;

#[test]
fn named_socket_listing_is_isolated() {
    let temp_home = TempHome::new("named-socket-listing");

    write_backup(
        temp_home.path(),
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/default-work"],
    );
    write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_20240102_120000",
        "ops",
        "2024-01-02 12:00:00",
        &["/srv/ops"],
    );

    let mut config = AppState::load_from_home(temp_home.path())
        .expect("runtime config should load from temp HOME");
    config.set_execution_options(ExecutionOptions::with_socket_name(Some("sockA")));

    let named_backups =
        catalog::list_backups(&config).expect("named-socket catalog listing should succeed");
    assert_eq!(
        named_backups
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        vec!["backup_20240102_120000"],
        "named-socket catalog should expose only active-root backups"
    );

    let summary_stdout = catalog::render_summary(&named_backups);
    assert!(
        summary_stdout.contains("backup_20240102_120000"),
        "named-socket summary should include active-root backup only: {summary_stdout}"
    );
    assert!(
        !summary_stdout.contains("backup_20240101_120000"),
        "named-socket summary must not leak default-root backup: {summary_stdout}"
    );

    let detail = run_binary(
        temp_home.path(),
        ["-L", "sockA", "-l", "backup_20240102_120000"],
    );
    assert!(detail.status.success(), "named detail failed: {detail:?}");
    let detail_stdout = String::from_utf8_lossy(&detail.stdout);
    assert!(detail_stdout.contains("Details of backup:backup_20240102_120000"));
    assert!(detail_stdout.contains("─Session─┬─[ops] (1 windows):"));
    assert!(detail_stdout.contains("─Pane (0) /srv/ops"));

    let missing_from_active_root = run_binary(
        temp_home.path(),
        ["-L", "sockA", "-l", "backup_20240101_120000"],
    );
    assert!(
        !missing_from_active_root.status.success(),
        "cross-root named lookup should fail"
    );
    let missing_stderr = String::from_utf8_lossy(&missing_from_active_root.stderr);
    assert!(
        missing_stderr.contains("cannot find given backup name:backup_20240101_120000"),
        "unexpected missing-name stderr: {missing_stderr}"
    );
}

#[test]
fn delete_named_backup_succeeds() {
    let temp_home = TempHome::new("delete-named-backup");
    let default_dir = write_backup(
        temp_home.path(),
        None,
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/default-work"],
    );
    let named_dir = write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_20240102_120000",
        "ops",
        "2024-01-02 12:00:00",
        &["/srv/ops"],
    );

    let delete = run_binary(
        temp_home.path(),
        ["-L", "sockA", "-d", "backup_20240102_120000"],
    );
    assert!(delete.status.success(), "named delete failed: {delete:?}");
    let delete_stdout = String::from_utf8_lossy(&delete.stdout);
    assert!(delete_stdout.contains("Backup backup_20240102_120000 was deleted"));
    assert!(!named_dir.exists(), "named backup should be removed");
    assert!(
        default_dir.exists(),
        "default-root backup must remain untouched"
    );
}

#[test]
fn delete_missing_backup_fails() {
    let temp_home = TempHome::new("delete-missing-backup");
    let preserved_dir = write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_20240102_120000",
        "ops",
        "2024-01-02 12:00:00",
        &["/srv/ops"],
    );

    let delete = run_binary(temp_home.path(), ["-L", "sockA", "-d", "missing_backup"]);
    assert!(
        !delete.status.success(),
        "missing named delete should return nonzero"
    );
    let delete_stderr = String::from_utf8_lossy(&delete.stderr);
    assert!(
        delete_stderr.contains("cannot find given backup name:missing_backup"),
        "unexpected missing-delete stderr: {delete_stderr}"
    );
    assert!(
        preserved_dir.exists(),
        "failed delete must not mutate existing backups"
    );
}

#[test]
fn latest_backup_resolution_is_deterministic() {
    let temp_home = TempHome::new("latest-backup-resolution");

    write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_20240101_120000",
        "work",
        "2024-01-01 12:00:00",
        &["/tmp/older"],
    );
    thread::sleep(Duration::from_millis(20));
    write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_20240102_120000",
        "ops",
        "2024-01-02 12:00:00",
        &["/tmp/newer"],
    );

    let mut config = AppState::load_from_home(temp_home.path())
        .expect("runtime config should load from temp HOME");
    config.set_execution_options(ExecutionOptions::with_socket_name(Some("sockA")));

    let latest = catalog::resolve_restore_target(&config, None)
        .expect("latest backup should resolve from active root");
    assert_eq!(latest, "backup_20240102_120000");
}

#[test]
fn catalog_named_ops_reuse_normalized_backup_names() {
    let temp_home = TempHome::new("catalog-normalized-names");
    let named_dir = write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_trimmed",
        "ops",
        "2024-01-02 12:00:00",
        &["/srv/ops"],
    );

    let mut config = AppState::load_from_home(temp_home.path())
        .expect("runtime config should load from temp HOME");
    config.set_execution_options(ExecutionOptions::with_socket_name(Some("sockA")));

    let loaded = catalog::load_backup(&config, "  backup_trimmed  ")
        .expect("catalog lookup should trim the requested backup name");
    assert_eq!(loaded.id, "backup_trimmed");

    let restore_target = catalog::resolve_restore_target(&config, Some("  backup_trimmed  "))
        .expect("restore target lookup should reuse normalized backup name");
    assert_eq!(restore_target, "backup_trimmed");

    catalog::delete_backup(&config, "  backup_trimmed  ")
        .expect("delete should reuse the same normalized backup name");
    assert!(
        !named_dir.exists(),
        "delete should remove the normalized backup directory"
    );
}

#[test]
fn named_lookup_reads_only_requested_backup() {
    let temp_home = TempHome::new("named-direct-load");
    write_backup(
        temp_home.path(),
        Some("sockA"),
        "backup_good",
        "ops",
        "2024-01-02 12:00:00",
        &["/srv/ops"],
    );

    let mut config = AppState::load_from_home(temp_home.path())
        .expect("runtime config should load from temp HOME");
    config.set_execution_options(ExecutionOptions::with_socket_name(Some("sockA")));

    let broken_dir = config.active_backup_path().join("backup_broken");
    fs::create_dir_all(&broken_dir).expect("broken backup directory should exist");
    fs::write(broken_dir.join("summary.json"), r#"{ "backup_id": 123 }"#)
        .expect("broken snapshot should be written");

    let loaded = catalog::load_backup(&config, "backup_good")
        .expect("named load should not scan unrelated broken backups");
    assert_eq!(loaded.id, "backup_good");
    assert_eq!(loaded.snapshot.sessions[0].name, "ops");
}

fn write_backup(
    home_dir: &Path,
    socket_name: Option<&str>,
    backup_id: &str,
    session_name: &str,
    create_time: &str,
    pane_paths: &[&str],
) -> PathBuf {
    let mut config =
        AppState::load_from_home(home_dir).expect("runtime config should bootstrap temp HOME");
    config.set_execution_options(ExecutionOptions::with_socket_name(socket_name));

    let backup_dir = config.active_backup_path().join(backup_id);
    fs::create_dir_all(&backup_dir).expect("backup directory should be created");

    let (tmux, pane_contents) =
        support::single_window_tmux(backup_id, session_name, create_time, pane_paths);
    snapshot::write_snapshot_dir(&backup_dir, &tmux, &pane_contents)
        .expect("snapshot directory should be written");

    backup_dir
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
            "remux-catalog-ops-{label}-{}-{unique}",
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
