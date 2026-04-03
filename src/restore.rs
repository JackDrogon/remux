use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backup_name::{BackupNameError, normalize_backup_name};
use crate::config::RuntimeConfig;
use crate::error::SubprocessError;
use crate::model::{Pane, Session, Tmux, Window};
use crate::serde_legacy::{self, LegacySnapshotError};
use crate::tmux::TmuxAdapter;

const DEFAULT_SESSION_SIZE: (u32, u32) = (10, 10);
const DUMMY_SESSION_SIZE: (u32, u32) = (10, 10);
const BASE_INDEX_OPTION: &str = "base-index";

#[derive(Debug)]
pub enum RestoreError {
    InvalidBackupName(BackupNameError),
    BackupRootRead {
        path: PathBuf,
        source: io::Error,
    },
    BackupMetadata {
        path: PathBuf,
        source: io::Error,
    },
    NoBackups {
        path: PathBuf,
    },
    BackupNotFound {
        name: String,
        path: PathBuf,
    },
    SnapshotLoad {
        path: PathBuf,
        source: LegacySnapshotError,
    },
    MissingPaneContent {
        pane_id: String,
        path: PathBuf,
    },
    InvalidBaseIndex {
        raw: String,
        source: std::num::ParseIntError,
    },
    Tmux(SubprocessError),
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBackupName(error) => write!(f, "{error}"),
            Self::BackupRootRead { path, source } => {
                write!(f, "failed to read backup root {}: {source}", path.display())
            }
            Self::BackupMetadata { path, source } => write!(
                f,
                "failed to inspect backup directory {}: {source}",
                path.display()
            ),
            Self::NoBackups { path } => write!(
                f,
                "(restore -r): backup dir is empty, nothing to restore: {}",
                path.display()
            ),
            Self::BackupNotFound { name, path } => write!(
                f,
                "(restore -r): cannot find given backup name:{name} under {}",
                path.display()
            ),
            Self::SnapshotLoad { path, source } => {
                write!(f, "failed to load snapshot {}: {source}", path.display())
            }
            Self::MissingPaneContent { pane_id, path } => {
                write!(f, "missing pane content for {pane_id}: {}", path.display())
            }
            Self::InvalidBaseIndex { raw, source } => {
                write!(f, "invalid tmux base-index value {raw:?}: {source}")
            }
            Self::Tmux(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for RestoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidBackupName(error) => Some(error),
            Self::BackupRootRead { source, .. } => Some(source),
            Self::BackupMetadata { source, .. } => Some(source),
            Self::SnapshotLoad { source, .. } => Some(source),
            Self::InvalidBaseIndex { source, .. } => Some(source),
            Self::Tmux(source) => Some(source),
            Self::NoBackups { .. }
            | Self::BackupNotFound { .. }
            | Self::MissingPaneContent { .. } => None,
        }
    }
}

impl From<SubprocessError> for RestoreError {
    fn from(value: SubprocessError) -> Self {
        Self::Tmux(value)
    }
}

pub fn restore_from_config(
    config: &RuntimeConfig,
    requested_backup: Option<&str>,
) -> Result<String, RestoreError> {
    let adapter = TmuxAdapter::new(config);
    let active_backup_path = config.active_backup_path();
    let backup_name = resolve_backup_name(&active_backup_path, requested_backup)?;
    restore_from_path_with_adapter(&active_backup_path, &adapter, &backup_name)?;
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
    adapter: &TmuxAdapter,
    backup_name: &str,
) -> Result<(), RestoreError> {
    let snapshot_path = snapshot_file_path(active_backup_path, backup_name);
    let tmux = serde_legacy::read_snapshot_file(&snapshot_path).map_err(|source| {
        RestoreError::SnapshotLoad {
            path: snapshot_path.clone(),
            source,
        }
    })?;

    let mut engine = RestoreEngine::new(active_backup_path, adapter);
    let restore_result = engine.restore_snapshot(&tmux, backup_name);
    let cleanup_result = engine.cleanup_dummy_session();

    restore_result?;
    cleanup_result?;
    Ok(())
}

fn snapshot_file_path(active_backup_path: &Path, backup_name: &str) -> PathBuf {
    active_backup_path
        .join(backup_name)
        .join(format!("{backup_name}.json"))
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

struct RestoreEngine<'a> {
    active_backup_path: &'a Path,
    adapter: &'a TmuxAdapter,
    win_base_index: Option<usize>,
    dummy_session: Option<String>,
}

impl<'a> RestoreEngine<'a> {
    fn new(active_backup_path: &'a Path, adapter: &'a TmuxAdapter) -> Self {
        Self {
            active_backup_path,
            adapter,
            win_base_index: None,
            dummy_session: None,
        }
    }

    fn restore_snapshot(&mut self, tmux: &Tmux, backup_name: &str) -> Result<(), RestoreError> {
        let sessions_to_restore = self.collect_restorable_sessions(tmux, backup_name)?;
        self.ensure_base_index_ready()?;

        for session in sessions_to_restore {
            self.restore_session(session, backup_name)?;
        }

        Ok(())
    }

    fn collect_restorable_sessions<'b>(
        &self,
        tmux: &'b Tmux,
        backup_name: &str,
    ) -> Result<Vec<&'b Session>, RestoreError> {
        let has_server = self.adapter.has_server()?;
        let mut sessions_to_restore = Vec::new();

        for session in &tmux.sessions {
            if has_server && self.adapter.has_session(&session.name)? {
                continue;
            }

            self.validate_session_assets(session, backup_name)?;
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
        backup_name: &str,
    ) -> Result<(), RestoreError> {
        let (width, height) = session.size.as_tuple().unwrap_or(DEFAULT_SESSION_SIZE);
        self.adapter.create_session(&session.name, width, height)?;

        let windows = session.windows_in_reverse();
        for window in windows.iter().take(windows.len().saturating_sub(1)) {
            self.restore_window(window, backup_name)?;
            self.adapter
                .create_empty_window(&session.name, self.ensure_base_index_ready()?)?;
        }

        if let Some(last_window) = windows.last() {
            self.restore_window(last_window, backup_name)?;
        }

        Ok(())
    }

    fn restore_window(&mut self, window: &Window, backup_name: &str) -> Result<(), RestoreError> {
        let win_base_index = self.ensure_base_index_ready()?;
        let window_id = usize::try_from(window.win_id)
            .expect("u32 window ids should always fit into usize on supported targets");

        if win_base_index != window_id {
            self.adapter
                .renumber_window(&window.sess_name, win_base_index, window_id)?;
        }

        self.adapter
            .rename_window(&window.sess_name, window_id, &window.name)?;

        if window.active {
            self.adapter.select_window(&window.sess_name, window_id)?;
        }

        if window.panes.len() > 1 {
            let pane_min_id = window
                .min_pane_id()
                .expect("multi-pane windows must expose a minimum pane id");
            let pane_min_id = usize::try_from(pane_min_id)
                .expect("u32 pane ids should always fit into usize on supported targets");
            for _ in 0..window.panes.len() - 1 {
                self.adapter
                    .split_window(&window.sess_name, window_id, pane_min_id)?;
            }
        }

        for pane in &window.panes {
            self.restore_pane(pane, backup_name)?;
        }

        self.adapter
            .select_layout(&window.sess_name, window_id, &window.layout)?;
        Ok(())
    }

    fn restore_pane(&self, pane: &Pane, backup_name: &str) -> Result<(), RestoreError> {
        let pane_id = pane.idstr();
        self.adapter
            .set_pane_path(&pane_id, Path::new(&pane.path))?;

        let content_path = self.pane_content_path(backup_name, &pane_id);
        self.ensure_pane_content_exists(&pane_id, &content_path)?;

        self.adapter
            .restore_pane_content(&pane.idstr(), &content_path)?;
        Ok(())
    }

    fn validate_session_assets(
        &self,
        session: &Session,
        backup_name: &str,
    ) -> Result<(), RestoreError> {
        for window in &session.windows {
            for pane in &window.panes {
                let pane_id = pane.idstr();
                let content_path = self.pane_content_path(backup_name, &pane_id);
                self.ensure_pane_content_exists(&pane_id, &content_path)?;
            }
        }

        Ok(())
    }

    fn pane_content_path(&self, backup_name: &str, pane_id: &str) -> PathBuf {
        self.active_backup_path.join(backup_name).join(pane_id)
    }

    fn ensure_pane_content_exists(
        &self,
        pane_id: &str,
        content_path: &Path,
    ) -> Result<(), RestoreError> {
        if content_path.is_file() {
            return Ok(());
        }

        Err(RestoreError::MissingPaneContent {
            pane_id: pane_id.to_string(),
            path: content_path.to_path_buf(),
        })
    }
}

fn generate_dummy_session_name() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("remux_dummy_{}_{}", std::process::id(), stamp)
}
