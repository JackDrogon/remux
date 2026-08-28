//! Capture the current tmux server state into the snapshot persistence format.
//!
//! This module intentionally keeps capture synchronous and staged: resolve the
//! backup identifier, read the tmux topology into the domain model, then record
//! pane bytes exactly as tmux emitted them before delegating the filesystem
//! contract to `snapshot.rs`. That ordering keeps the persisted snapshot stable
//! and avoids mixing filesystem concerns into the capture path.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{AppState, ExecutionOptions};
use crate::error::{Backup as BackupError, Error, Result};
use crate::model::{Pane, Session, Size, Tmux, Window};
use crate::storage::{BackupId, write_snapshot_dir};
use crate::tmux_adapter::{
    OUTPUT_SEPARATOR, TmuxClient, TmuxRuntimeOptions, discover_socket_names,
};
use chrono::{Local, NaiveDateTime};
use xerror::Context;

mod process;

const CREATE_TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackupTimestamp {
    local_time: NaiveDateTime,
}

impl BackupTimestamp {
    fn now() -> Self {
        Self {
            local_time: Local::now().naive_local(),
        }
    }

    fn backup_id(self) -> String {
        BackupId::automatic_at(self.local_time).into_string()
    }

    fn create_time(self) -> String {
        format_local_time(self.local_time, CREATE_TIME_FORMAT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupOutcome {
    Created { backup_id: String, path: PathBuf },
    NoServer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketBackupOutcome {
    pub socket_name: String,
    pub outcome: BackupOutcome,
}

/// Per-socket capture result for a multi-socket backup.
///
/// Kept as a list instead of `Result<Vec<_>>` so a later socket failure does
/// not discard backups that already landed on disk.
#[derive(Debug)]
pub enum SocketBackupResult {
    Completed(SocketBackupOutcome),
    Failed { socket_name: String, error: Error },
}

pub fn capture_backup(
    config: &AppState,
    requested_backup_id: Option<&str>,
) -> Result<BackupOutcome> {
    tracing::info!(
        requested_backup_id = requested_backup_id.unwrap_or("-"),
        socket_name = config.socket_name().unwrap_or("default"),
        backup_root = %config.active_backup_path().display(),
        "starting backup capture"
    );
    let adapter = TmuxRuntimeOptions::new(&config.config().tmux.binary)
        .socket_name(config.socket_name())
        .content_with_escape(config.config().capture.with_escape)
        .build_adapter();
    capture_backup_with_client(config, requested_backup_id, &adapter)
}

/// Capture every discovered tmux socket, continuing after per-socket errors.
///
/// Discovery itself is still fatal: if the socket directory cannot be read,
/// nothing has been written yet. An empty directory falls back to the unnamed
/// default server so a machine with no leftover sockets still backs up `tmux`
/// without `-L`.
pub fn capture_all_socket_backups(config: &AppState) -> Result<Vec<SocketBackupResult>> {
    let socket_names = discover_socket_names()?;

    if socket_names.is_empty() {
        return capture_unnamed_default_socket(config);
    }

    Ok(socket_names
        .into_iter()
        .map(|socket_name| capture_backup_for_discovered_socket(config, socket_name))
        .collect())
}

fn capture_unnamed_default_socket(config: &AppState) -> Result<Vec<SocketBackupResult>> {
    capture_backup(config, None).map(|outcome| {
        vec![SocketBackupResult::Completed(SocketBackupOutcome {
            socket_name: "default".to_string(),
            outcome,
        })]
    })
}

fn capture_backup_for_discovered_socket(
    config: &AppState,
    socket_name: String,
) -> SocketBackupResult {
    let mut socket_config = config.clone();
    // A filesystem entry named `default` is the unnamed tmux server. Passing
    // `-L default` would talk to the same server but write a different catalog.
    let socket_name_for_tmux = (socket_name != "default").then_some(socket_name.as_str());
    socket_config.set_execution_options(ExecutionOptions::with_socket_name(socket_name_for_tmux));
    match capture_backup(&socket_config, None).with_context(|| format!("socket {socket_name}")) {
        Ok(outcome) => SocketBackupResult::Completed(SocketBackupOutcome {
            socket_name,
            outcome,
        }),
        Err(error) => SocketBackupResult::Failed { socket_name, error },
    }
}

fn capture_backup_with_client(
    config: &AppState,
    requested_backup_id: Option<&str>,
    client: &impl TmuxClient,
) -> Result<BackupOutcome> {
    let timestamp = BackupTimestamp::now();
    let backup_id = resolve_backup_id(requested_backup_id, timestamp)?;
    let backup_path = config.active_backup_path().join(&backup_id);

    tracing::info!(backup_id, path = %backup_path.display(), "resolved backup destination");

    // Preflight only: occupied names skip capture. This is not a lock; a race
    // at commit stays Snapshot I/O and is not reclassified as Duplicate.
    if crate::storage::catalog::backup_id_slot_is_occupied(&backup_path)? {
        return Err(BackupError::DuplicateBackupId {
            backup_id,
            path: backup_path,
        }
        .into());
    }

    if !client.has_server()? {
        tracing::info!("skipping backup because no tmux server is running");
        return Ok(BackupOutcome::NoServer);
    }

    let snapshot = load_snapshot(client, &backup_id, timestamp)?;
    let pane_contents = capture_snapshot_panes(client, &snapshot)?;
    let session_count = snapshot.sessions.len();
    let window_count = snapshot
        .sessions
        .iter()
        .map(|session| session.windows.len())
        .sum::<usize>();

    write_snapshot_dir(&backup_path, &snapshot, &pane_contents)?;

    tracing::info!(
        backup_id = %snapshot.backup_id,
        path = %backup_path.display(),
        session_count,
        window_count,
        pane_count = pane_contents.len(),
        "backup snapshot written"
    );

    Ok(BackupOutcome::Created {
        backup_id,
        path: backup_path,
    })
}

fn resolve_backup_id(
    requested_backup_id: Option<&str>,
    timestamp: BackupTimestamp,
) -> Result<String> {
    match requested_backup_id {
        Some(backup_id) => Ok(BackupId::parse_custom(backup_id)?.into_string()),
        None => Ok(timestamp.backup_id()),
    }
}

fn load_snapshot(
    client: &impl TmuxClient,
    backup_id: &str,
    timestamp: BackupTimestamp,
) -> Result<Tmux> {
    Ok(Tmux {
        backup_id: backup_id.to_string(),
        sessions: load_sessions(client)?,
        create_time: timestamp.create_time(),
    })
}

fn capture_snapshot_panes(
    client: &impl TmuxClient,
    snapshot: &Tmux,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut pane_contents = BTreeMap::new();

    for pane in snapshot.panes() {
        let pane_id = pane.pane_target().into_string();
        let pane_bytes = client.capture_pane_bytes(&pane_id)?;
        tracing::debug!(pane_id, byte_len = pane_bytes.len(), "captured pane bytes");
        pane_contents.insert(pane_id, pane_bytes);
    }

    Ok(pane_contents)
}

fn load_sessions(client: &impl TmuxClient) -> Result<Vec<Session>> {
    client
        .list_sessions()?
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| parse_session(client, &line))
        .collect()
}

fn parse_session(client: &impl TmuxClient, line: &str) -> Result<Session> {
    let fields = split_fields("list-sessions", line, 3)?;
    let name = fields[0].to_string();
    Ok(Session {
        windows: load_windows(client, &name)?,
        name,
        attached: parse_active(fields[2], "list-sessions", line)?,
        size: parse_size(fields[1], "list-sessions", line)?,
    })
}

fn load_windows(client: &impl TmuxClient, session_name: &str) -> Result<Vec<Window>> {
    client
        .list_windows(session_name)?
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| parse_window(client, session_name, &line))
        .collect()
}

fn parse_window(client: &impl TmuxClient, session_name: &str, line: &str) -> Result<Window> {
    let fields = split_fields("list-windows", line, 4)?;
    let window_id = parse_u32(fields[0], "list-windows", line)?;
    Ok(Window {
        window_id,
        name: fields[1].to_string(),
        panes: load_panes(client, session_name, window_id)?,
        active: parse_active(fields[2], "list-windows", line)?,
        session_name: session_name.to_string(),
        layout: fields[3].to_string(),
    })
}

fn load_panes(client: &impl TmuxClient, session_name: &str, window_id: u32) -> Result<Vec<Pane>> {
    client
        .list_panes(
            session_name,
            usize::try_from(window_id).map_err(|_| {
                invalid_tmux_output(
                    "list-panes",
                    format!("{session_name}:{window_id}"),
                    "window id exceeds usize range",
                )
            })?,
        )?
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| parse_pane(session_name, window_id, &line))
        .collect()
}

fn parse_pane(session_name: &str, window_id: u32, line: &str) -> Result<Pane> {
    let fields = split_fields("list-panes", line, 6)?;
    let pane_id = parse_u32(fields[0], "list-panes", line)?;
    Ok(Pane {
        pane_id,
        size: parse_size(fields[1], "list-panes", line)?,
        path: fields[2].to_string(),
        active: parse_active(fields[3], "list-panes", line)?,
        session_name: session_name.to_string(),
        window_id,
        command_tree: pane_command_tree_from_tmux(fields[4], fields[5], line)?,
    })
}

fn pane_command_tree_from_tmux(
    current_command: &str,
    pane_pid: &str,
    line: &str,
) -> Result<Option<crate::model::Process>> {
    let pane_pid = pane_pid.trim();
    if pane_pid.is_empty() {
        return Ok(None);
    }

    let pane_pid = parse_u32(pane_pid, "list-panes", line)?;
    if pane_pid == 0 {
        return Ok(None);
    }

    Ok(process::inspect_command_tree(
        pane_pid,
        current_command.trim(),
    ))
}

fn split_fields<'a>(
    command: &'static str,
    line: &'a str,
    expected_len: usize,
) -> Result<Vec<&'a str>> {
    let fields = line.split(OUTPUT_SEPARATOR).collect::<Vec<_>>();
    if fields.len() == expected_len {
        Ok(fields)
    } else {
        Err(invalid_tmux_output(
            command,
            line.to_string(),
            format!("expected {expected_len} fields, found {}", fields.len()),
        ))
    }
}

fn parse_size(command_value: &str, command: &'static str, line: &str) -> Result<Size> {
    let Some(inner) = command_value
        .trim()
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Err(invalid_size_tuple(command, line, command_value));
    };

    if inner.trim().is_empty() {
        return Ok(Size::empty());
    }

    let Some((width, height)) = inner.split_once(',') else {
        return Err(invalid_size_tuple(command, line, command_value));
    };

    Ok(Size::new(
        parse_u32(width.trim(), command, line)?,
        parse_u32(height.trim(), command, line)?,
    ))
}

fn parse_u32(value: &str, command: &'static str, line: &str) -> Result<u32> {
    value.parse::<u32>().map_err(|error| {
        invalid_tmux_output(
            command,
            line.to_string(),
            format!("failed to parse integer {value:?}: {error}"),
        )
    })
}

fn parse_active(value: &str, command: &'static str, line: &str) -> Result<bool> {
    value
        .parse::<i64>()
        .map(|value| value > 0)
        .map_err(|error| {
            invalid_tmux_output(
                command,
                line.to_string(),
                format!("failed to parse active flag {value:?}: {error}"),
            )
        })
}

fn invalid_size_tuple(command: &'static str, line: &str, command_value: &str) -> Error {
    invalid_tmux_output(
        command,
        line.to_string(),
        format!("invalid size tuple: {command_value}"),
    )
}

fn invalid_tmux_output(command: &'static str, line: String, detail: impl Into<String>) -> Error {
    BackupError::InvalidTmuxOutput {
        command,
        line,
        detail: detail.into(),
    }
    .into()
}

fn format_local_time(local_time: NaiveDateTime, format: &str) -> String {
    local_time.format(format).to_string()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use chrono::NaiveDate;

    use super::*;
    use crate::config::AppState;
    use crate::error::Tmux as TmuxError;

    #[test]
    fn capture_backup_with_client_uses_trait_fake() {
        let temp_home = TempHome::new("backup-client-fake");
        let config = AppState::load_from_home(temp_home.path())
            .expect("config bootstrap should succeed for backup fake test");
        let fake = FakeBackupClient::with_capture(
            vec!["work:=:(120,40):=:0".to_string()],
            BTreeMap::from([(
                "work".to_string(),
                vec!["1:=:editor:=:1:=:1900,120x40,0,0,0".to_string()],
            )]),
            BTreeMap::from([(
                ("work".to_string(), 1usize),
                vec!["0:=:(120,40):=:/tmp/work:=:1:=:zsh:=:1".to_string()],
            )]),
            BTreeMap::from([("work:1.0".to_string(), b"pane0\n".to_vec())]),
        );

        let outcome = capture_backup_with_client(&config, Some("trait_fake"), &fake)
            .expect("backup should succeed with trait fake");

        match outcome {
            BackupOutcome::Created { backup_id, path } => {
                assert_eq!(backup_id, "trait_fake");
                assert!(path.join("summary.json").is_file());
                assert!(path.join("manifest.json").is_file());
                assert!(path.join("panes/work:1.0.txt").is_file());
            }
            BackupOutcome::NoServer => panic!("expected backup creation, got no server"),
        }
    }

    #[test]
    fn format_local_time_matches_backup_id_shape() {
        let local_time = NaiveDate::from_ymd_opt(2024, 1, 2)
            .expect("test date should be valid")
            .and_hms_opt(3, 4, 5)
            .expect("test time should be valid");

        assert_eq!(
            BackupId::automatic_at(local_time).as_str(),
            "20240102_030405"
        );
    }

    #[test]
    fn format_local_time_matches_snapshot_shape() {
        let local_time = NaiveDate::from_ymd_opt(2024, 1, 2)
            .expect("test date should be valid")
            .and_hms_opt(3, 4, 5)
            .expect("test time should be valid");

        assert_eq!(
            format_local_time(local_time, CREATE_TIME_FORMAT),
            "2024-01-02 03:04:05"
        );
    }

    #[test]
    fn backup_timestamp_formats_both_views_from_one_time() {
        let timestamp = BackupTimestamp {
            local_time: NaiveDate::from_ymd_opt(2024, 1, 2)
                .expect("test date should be valid")
                .and_hms_opt(3, 4, 5)
                .expect("test time should be valid"),
        };

        assert_eq!(timestamp.backup_id(), "20240102_030405");
        assert_eq!(timestamp.create_time(), "2024-01-02 03:04:05");
    }

    struct FakeBackupClient {
        sessions: Vec<String>,
        windows: BTreeMap<String, Vec<String>>,
        panes: BTreeMap<(String, usize), Vec<String>>,
        captures: BTreeMap<String, Vec<u8>>,
    }

    impl FakeBackupClient {
        fn with_capture(
            sessions: Vec<String>,
            windows: BTreeMap<String, Vec<String>>,
            panes: BTreeMap<(String, usize), Vec<String>>,
            captures: BTreeMap<String, Vec<u8>>,
        ) -> Self {
            Self {
                sessions,
                windows,
                panes,
                captures,
            }
        }
    }

    impl TmuxClient for FakeBackupClient {
        fn has_server(&self) -> Result<bool> {
            Ok(true)
        }

        fn list_sessions(&self) -> Result<Vec<String>> {
            Ok(self.sessions.clone())
        }

        fn list_windows(&self, session_name: &str) -> Result<Vec<String>> {
            Ok(self.windows.get(session_name).cloned().unwrap_or_default())
        }

        fn list_panes(&self, session_name: &str, window_index: usize) -> Result<Vec<String>> {
            Ok(self
                .panes
                .get(&(session_name.to_string(), window_index))
                .cloned()
                .unwrap_or_default())
        }

        fn create_session(&self, _session_name: &str, _width: u32, _height: u32) -> Result<()> {
            unreachable!("backup fake should not create sessions")
        }

        fn kill_session(&self, _session_name: &str) -> Result<bool> {
            unreachable!("backup fake should not kill sessions")
        }

        fn capture_pane(&self, _pane_id: &str) -> Result<String> {
            unreachable!("backup fake should use byte capture")
        }

        fn capture_pane_bytes(&self, pane_id: &str) -> Result<Vec<u8>> {
            self.captures.get(pane_id).cloned().ok_or_else(|| {
                TmuxError::TmuxFailed {
                    command: vec![
                        "fake".to_string(),
                        "capture-pane".to_string(),
                        pane_id.to_string(),
                    ],
                    status: Some(1),
                    stdout: String::new(),
                    stderr: "missing pane capture".to_string(),
                }
                .into()
            })
        }

        fn show_option(&self, _option: &str) -> Result<String> {
            unreachable!("backup fake should not read options")
        }

        fn has_session(&self, _session_name: &str) -> Result<bool> {
            unreachable!("backup fake should not probe sessions individually")
        }

        fn clear_pane(&self, _pane_id: &str) -> Result<()> {
            unreachable!("backup fake should not mutate panes")
        }

        fn send_keys(&self, _target: &str, _keys: &str) -> Result<()> {
            unreachable!("backup fake should not send keys")
        }

        fn create_empty_window(&self, _session_name: &str, _base_index: usize) -> Result<()> {
            unreachable!("backup fake should not create windows")
        }

        fn move_window(&self, _source: &str, _target: &str) -> Result<()> {
            unreachable!("backup fake should not move windows")
        }

        fn rename_window(&self, _session_name: &str, _window_id: usize, _name: &str) -> Result<()> {
            unreachable!("backup fake should not rename windows")
        }

        fn select_window(&self, _session_name: &str, _window_id: usize) -> Result<()> {
            unreachable!("backup fake should not select windows")
        }

        fn split_window(
            &self,
            _session_name: &str,
            _window_id: usize,
            _pane_min_id: usize,
        ) -> Result<()> {
            unreachable!("backup fake should not split windows")
        }

        fn select_layout(
            &self,
            _session_name: &str,
            _window_id: usize,
            _layout: &str,
        ) -> Result<()> {
            unreachable!("backup fake should not select layouts")
        }

        fn restore_pane_content(&self, _pane_id: &str, _filename: &Path) -> Result<()> {
            unreachable!("backup fake should not restore pane content")
        }
    }

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
                "remux-backup-test-{label}-{}-{unique}",
                std::process::id()
            ));
            if path.exists() {
                fs::remove_dir_all(&path).expect("stale temp HOME should be removable");
            }
            fs::create_dir_all(&path).expect("temp HOME should be created");
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
}
