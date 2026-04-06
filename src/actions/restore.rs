//! Replay a persisted snapshot back into a live tmux server.
//!
//! The restore path validates pane assets before mutating tmux on purpose.
//! Snapshot reads are cheap to repeat, but tmux mutations are not transactional,
//! so this module keeps the "validate first, replay second" boundary explicit
//! to preserve fail-fast behavior and predictable recovery semantics.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::backup_name::{BackupNameError, normalize_backup_name};
use crate::config::AppState;
use crate::error::SubprocessError;
use crate::hash::sha256_hex;
use crate::model::{Pane, Session, Tmux, Window};
use crate::storage::{LoadedSnapshot, PaneAsset, SnapshotError, read_snapshot_dir};
use crate::tmux::{TmuxClient, TmuxRuntimeOptions};

const DEFAULT_SESSION_SIZE: (u32, u32) = (10, 10);
const DUMMY_SESSION_SIZE: (u32, u32) = (10, 10);
const BASE_INDEX_OPTION: &str = "base-index";

#[derive(Debug, Error)]
pub enum RestoreError {
    #[error(transparent)]
    InvalidBackupName(#[from] BackupNameError),
    #[error("failed to read backup root {}: {source}", path.display())]
    BackupRootRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to inspect backup directory {}: {source}", path.display())]
    BackupMetadata {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("backup directory is empty, nothing to restore: {}", path.display())]
    NoBackups { path: PathBuf },
    #[error("cannot find given backup name:{name} under {}", path.display())]
    BackupNotFound { name: String, path: PathBuf },
    #[error("failed to load snapshot {}: {source}", path.display())]
    SnapshotLoad {
        path: PathBuf,
        #[source]
        source: SnapshotError,
    },
    #[error("missing pane content for {pane_id}: {}", path.display())]
    MissingPaneContent { pane_id: String, path: PathBuf },
    #[error("missing pane metadata for {pane_id}")]
    MissingPaneAsset { pane_id: String },
    #[error("invalid pane content for {pane_id} at {}: {detail}", path.display())]
    InvalidPaneContent {
        pane_id: String,
        path: PathBuf,
        detail: String,
    },
    #[error("invalid tmux base-index value {raw:?}: {source}")]
    InvalidBaseIndex {
        raw: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error(transparent)]
    Tmux(#[from] SubprocessError),
}

pub fn restore_from_config(
    config: &AppState,
    requested_backup: Option<&str>,
) -> Result<String, RestoreError> {
    tracing::info!(
        requested_backup = requested_backup.unwrap_or("latest"),
        socket_name = config.socket_name().unwrap_or("default"),
        backup_root = %config.active_backup_path().display(),
        "starting restore"
    );
    let adapter = TmuxRuntimeOptions::new(&config.config().tmux.binary)
        .socket_name(config.socket_name())
        .content_with_escape(config.config().capture.with_escape)
        .build_adapter();
    let active_backup_path = config.active_backup_path();
    let backup_name = resolve_backup_name(&active_backup_path, requested_backup)?;
    restore_from_path_with_adapter(&active_backup_path, &adapter, &backup_name)?;
    tracing::info!(backup_name, "restore completed");
    Ok(backup_name)
}

pub fn resolve_backup_name(
    active_backup_path: &Path,
    requested_backup: Option<&str>,
) -> Result<String, RestoreError> {
    let requested_backup = requested_backup
        .map(|requested_backup| {
            normalize_backup_name(requested_backup).map_err(RestoreError::InvalidBackupName)
        })
        .transpose()?;

    let backups = list_backups(active_backup_path)?;
    if backups.is_empty() {
        return Err(RestoreError::NoBackups {
            path: active_backup_path.to_path_buf(),
        });
    }

    if let Some(requested_backup) = requested_backup {
        if backups.iter().any(|backup| backup.name == requested_backup) {
            return Ok(requested_backup);
        }

        return Err(RestoreError::BackupNotFound {
            name: requested_backup,
            path: active_backup_path.to_path_buf(),
        });
    }

    backups
        .into_iter()
        .max_by_key(|backup| (backup.modified, backup.name.clone()))
        .map(|backup| backup.name)
        .ok_or_else(|| RestoreError::NoBackups {
            path: active_backup_path.to_path_buf(),
        })
}

pub fn restore_from_path_with_adapter(
    active_backup_path: &Path,
    adapter: &impl TmuxClient,
    backup_name: &str,
) -> Result<(), RestoreError> {
    let backup_dir = backup_dir_path(active_backup_path, backup_name);
    let snapshot = read_snapshot_dir(&backup_dir).map_err(|source| RestoreError::SnapshotLoad {
        path: backup_dir.clone(),
        source,
    })?;

    let mut engine = RestoreEngine::new(adapter);
    let restore_result = engine.restore_snapshot(&snapshot, &backup_dir);
    let cleanup_result = engine.cleanup_dummy_session();

    restore_result?;
    cleanup_result?;
    Ok(())
}

fn backup_dir_path(active_backup_path: &Path, backup_name: &str) -> PathBuf {
    active_backup_path.join(backup_name)
}

fn list_backups(active_backup_path: &Path) -> Result<Vec<BackupEntry>, RestoreError> {
    if !active_backup_path.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(active_backup_path).map_err(|source| RestoreError::BackupRootRead {
            path: active_backup_path.to_path_buf(),
            source,
        })?;

    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| RestoreError::BackupRootRead {
            path: active_backup_path.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|source| RestoreError::BackupMetadata {
                path: path.clone(),
                source,
            })?;

        if !metadata.is_dir() {
            continue;
        }

        backups.push(BackupEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            modified: metadata
                .modified()
                .map_err(|source| RestoreError::BackupMetadata { path, source })?,
        });
    }

    Ok(backups)
}

#[derive(Debug)]
struct BackupEntry {
    name: String,
    modified: SystemTime,
}

struct RestoreEngine<'a, T: TmuxClient + ?Sized> {
    adapter: &'a T,
    win_base_index: Option<usize>,
    dummy_session: Option<String>,
}

#[derive(Debug, Default)]
struct VerifiedPaneAssets {
    content_paths: BTreeMap<String, PathBuf>,
}

impl VerifiedPaneAssets {
    fn insert(&mut self, pane_id: String, content_path: PathBuf) {
        self.content_paths.insert(pane_id, content_path);
    }

    fn content_path(&self, pane_id: &str) -> Result<&Path, RestoreError> {
        self.content_paths
            .get(pane_id)
            .map(PathBuf::as_path)
            .ok_or_else(|| RestoreError::MissingPaneAsset {
                pane_id: pane_id.to_string(),
            })
    }
}

impl<'a, T: TmuxClient + ?Sized> RestoreEngine<'a, T> {
    fn new(adapter: &'a T) -> Self {
        Self {
            adapter,
            win_base_index: None,
            dummy_session: None,
        }
    }

    fn restore_snapshot(
        &mut self,
        snapshot: &LoadedSnapshot,
        backup_dir: &Path,
    ) -> Result<(), RestoreError> {
        tracing::info!(
            backup_dir = %backup_dir.display(),
            session_count = snapshot.tmux.sessions.len(),
            pane_asset_count = snapshot.pane_assets.len(),
            "restoring snapshot into tmux"
        );
        let sessions_to_restore = self.collect_restorable_sessions(&snapshot.tmux)?;
        let verified_panes =
            self.validate_sessions(&sessions_to_restore, &snapshot.pane_assets, backup_dir)?;
        self.ensure_base_index_ready()?;

        tracing::info!(
            session_count = sessions_to_restore.len(),
            pane_asset_count = verified_panes.content_paths.len(),
            "validated restore inputs"
        );

        for session in sessions_to_restore {
            self.restore_session(session, &verified_panes)?;
        }

        Ok(())
    }

    fn collect_restorable_sessions<'b>(
        &self,
        tmux: &'b Tmux,
    ) -> Result<Vec<&'b Session>, RestoreError> {
        let has_server = self.adapter.has_server()?;
        let mut sessions_to_restore = Vec::new();

        for session in &tmux.sessions {
            if has_server && self.adapter.has_session(&session.name)? {
                tracing::debug!(session_name = %session.name, "skipping existing tmux session");
                continue;
            }

            sessions_to_restore.push(session);
        }

        Ok(sessions_to_restore)
    }

    fn cleanup_dummy_session(&mut self) -> Result<(), RestoreError> {
        if let Some(dummy_session) = self.dummy_session.take() {
            self.adapter.kill_session(&dummy_session)?;
        }

        Ok(())
    }

    fn ensure_base_index_ready(&mut self) -> Result<usize, RestoreError> {
        if let Some(win_base_index) = self.win_base_index {
            return Ok(win_base_index);
        }

        if !self.adapter.has_server()? {
            let dummy_session = generate_dummy_session_name();
            self.adapter.create_session(
                &dummy_session,
                DUMMY_SESSION_SIZE.0,
                DUMMY_SESSION_SIZE.1,
            )?;
            self.dummy_session = Some(dummy_session);
        }

        let raw = self.adapter.show_option(BASE_INDEX_OPTION)?;
        let win_base_index = raw
            .trim()
            .parse::<usize>()
            .map_err(|source| RestoreError::InvalidBaseIndex { raw, source })?;
        self.win_base_index = Some(win_base_index);
        Ok(win_base_index)
    }

    fn restore_session(
        &mut self,
        session: &Session,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<(), RestoreError> {
        let (width, height) = session.size.as_tuple().unwrap_or(DEFAULT_SESSION_SIZE);
        self.adapter.create_session(&session.name, width, height)?;

        let windows = session.windows_in_reverse();
        for window in windows.iter().take(windows.len().saturating_sub(1)) {
            self.restore_window(window, verified_panes)?;
            self.adapter
                .create_empty_window(&session.name, self.ensure_base_index_ready()?)?;
        }

        if let Some(last_window) = windows.last() {
            self.restore_window(last_window, verified_panes)?;
        }

        Ok(())
    }

    fn restore_window(
        &mut self,
        window: &Window,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<(), RestoreError> {
        let win_base_index = self.ensure_base_index_ready()?;
        let window_id = window_id(window);

        self.restore_window_identity(window, win_base_index, window_id)?;
        self.restore_window_panes(window, window_id, verified_panes)?;
        self.adapter
            .select_layout(&window.sess_name, window_id, &window.layout)?;
        Ok(())
    }

    fn restore_window_identity(
        &self,
        window: &Window,
        win_base_index: usize,
        window_id: usize,
    ) -> Result<(), RestoreError> {
        if win_base_index != window_id {
            self.adapter
                .renumber_window(&window.sess_name, win_base_index, window_id)?;
        }

        self.adapter
            .rename_window(&window.sess_name, window_id, &window.name)?;

        if window.active {
            self.adapter.select_window(&window.sess_name, window_id)?;
        }

        Ok(())
    }

    fn restore_window_panes(
        &self,
        window: &Window,
        window_id: usize,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<(), RestoreError> {
        self.expand_window_panes(window, window_id)?;

        for pane in &window.panes {
            self.restore_pane(pane, verified_panes)?;
        }

        Ok(())
    }

    fn expand_window_panes(&self, window: &Window, window_id: usize) -> Result<(), RestoreError> {
        if window.panes.len() <= 1 {
            return Ok(());
        }

        let pane_min_id = pane_min_id(window);
        for _ in 0..window.panes.len() - 1 {
            self.adapter
                .split_window(&window.sess_name, window_id, pane_min_id)?;
        }

        Ok(())
    }

    fn restore_pane(
        &self,
        pane: &Pane,
        verified_panes: &VerifiedPaneAssets,
    ) -> Result<(), RestoreError> {
        let pane_id = pane.idstr();
        self.adapter
            .set_pane_path(&pane_id, Path::new(&pane.path))?;

        let content_path = verified_panes.content_path(&pane_id)?;

        self.adapter.restore_pane_content(&pane_id, content_path)?;
        Ok(())
    }

    fn validate_sessions(
        &self,
        sessions: &[&Session],
        pane_assets: &BTreeMap<String, PaneAsset>,
        backup_dir: &Path,
    ) -> Result<VerifiedPaneAssets, RestoreError> {
        let mut verified = VerifiedPaneAssets::default();

        for session in sessions {
            for window in &session.windows {
                for pane in &window.panes {
                    let pane_id = pane.idstr();
                    let content_path =
                        self.validated_pane_content_path(&pane_id, pane_assets, backup_dir)?;
                    verified.insert(pane_id, content_path);
                }
            }
        }

        Ok(verified)
    }

    fn validated_pane_content_path(
        &self,
        pane_id: &str,
        pane_assets: &BTreeMap<String, PaneAsset>,
        backup_dir: &Path,
    ) -> Result<PathBuf, RestoreError> {
        let asset = pane_assets
            .get(pane_id)
            .ok_or_else(|| RestoreError::MissingPaneAsset {
                pane_id: pane_id.to_string(),
            })?;
        let content_path = backup_dir.join(&asset.relative_path);

        if !content_path.is_file() {
            return Err(RestoreError::MissingPaneContent {
                pane_id: pane_id.to_string(),
                path: content_path,
            });
        }

        let bytes = fs::read(&content_path).map_err(|source| RestoreError::BackupMetadata {
            path: content_path.clone(),
            source,
        })?;
        if bytes.len() as u64 != asset.byte_len {
            return Err(invalid_pane_content(
                pane_id,
                content_path,
                format!("expected {} bytes, found {}", asset.byte_len, bytes.len()),
            ));
        }

        let actual_hash = sha256_hex(&bytes);
        if actual_hash != asset.sha256 {
            return Err(invalid_pane_content(
                pane_id,
                content_path,
                format!("expected sha256 {}, found {}", asset.sha256, actual_hash),
            ));
        }

        Ok(content_path)
    }
}

fn invalid_pane_content(pane_id: &str, path: PathBuf, detail: impl Into<String>) -> RestoreError {
    RestoreError::InvalidPaneContent {
        pane_id: pane_id.to_string(),
        path,
        detail: detail.into(),
    }
}

fn window_id(window: &Window) -> usize {
    usize::try_from(window.win_id)
        .expect("u32 window ids should always fit into usize on supported targets")
}

fn pane_min_id(window: &Window) -> usize {
    let pane_min_id = window
        .min_pane_id()
        .expect("multi-pane windows must expose a minimum pane id");
    usize::try_from(pane_min_id)
        .expect("u32 pane ids should always fit into usize on supported targets")
}

fn generate_dummy_session_name() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("remux_dummy_{}_{}", std::process::id(), stamp)
}
