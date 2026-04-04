//! Catalog views over persisted backup directories.
//!
//! Listing, lookup, rendering, and deletion live together so every CLI path
//! applies the same root-isolation and snapshot-decoding rules. The sort order
//! is intentionally explicit because the interactive list and the default
//! restore target use different stability requirements.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use thiserror::Error;

use super::catalog_render;
use super::fs_ops;
use super::snapshot::{self, SnapshotError};
use crate::backup_name::{BackupNameError, normalize_backup_name};
use crate::config::AppState;
use crate::model::Tmux;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackupSortOrder {
    ModifiedAtDesc,
    BackupIdDesc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotLoadMode {
    Full,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    pub id: String,
    pub path: PathBuf,
    pub modified_at: Duration,
    pub snapshot: Tmux,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error(transparent)]
    InvalidBackupName(#[from] BackupNameError),
    #[error("failed to read backup catalog {}: {source}", path.display())]
    ReadCatalog {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read backup metadata {}: {source}", path.display())]
    ReadMetadata {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read backup snapshot {}: {source}", path.display())]
    ReadSnapshot {
        path: PathBuf,
        #[source]
        source: SnapshotError,
    },
    #[error("cannot find given backup name:{name} under {}", root.display())]
    MissingBackupName { name: String, root: PathBuf },
    #[error("backup dir is empty under {}, nothing to resolve", root.display())]
    NoBackups { root: PathBuf },
    #[error("failed to delete backup {}: {source}", path.display())]
    DeleteBackup {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub fn list_backups(config: &AppState) -> Result<Vec<BackupEntry>, CatalogError> {
    load_backups(
        config,
        BackupSortOrder::ModifiedAtDesc,
        SnapshotLoadMode::Full,
    )
}

pub fn list_backups_for_listing(config: &AppState) -> Result<Vec<BackupEntry>, CatalogError> {
    load_backups(
        config,
        BackupSortOrder::BackupIdDesc,
        SnapshotLoadMode::Summary,
    )
}

pub fn load_backup(config: &AppState, backup_name: &str) -> Result<BackupEntry, CatalogError> {
    let normalized_name =
        normalize_backup_name(backup_name).map_err(CatalogError::InvalidBackupName)?;
    let root = config.active_backup_path();
    read_backup_entry_in_root(&root, &normalized_name)
}

pub fn latest_backup(config: &AppState) -> Result<BackupEntry, CatalogError> {
    let root = config.active_backup_path();
    list_backups(config)?
        .into_iter()
        .next()
        .ok_or(CatalogError::NoBackups { root })
}

pub fn resolve_restore_target(
    config: &AppState,
    requested_name: Option<&str>,
) -> Result<String, CatalogError> {
    match requested_name {
        Some(requested_name) => Ok(load_backup(config, requested_name)?.id),
        _ => Ok(latest_backup(config)?.id),
    }
}

pub fn delete_backup(config: &AppState, backup_name: &str) -> Result<(), CatalogError> {
    let entry = load_backup(config, backup_name)?;
    fs_ops::remove_dir_all(&entry.path, |path, source| CatalogError::DeleteBackup {
        path,
        source,
    })
}

pub fn no_backups_message() -> &'static str {
    catalog_render::no_backups_message()
}

pub fn render_summary(backups: &[BackupEntry]) -> String {
    catalog_render::render_summary(backups)
}

pub fn render_detail(entry: &BackupEntry) -> String {
    catalog_render::render_detail(entry)
}

pub fn render_interactive_detail(entry: &BackupEntry) -> String {
    catalog_render::render_interactive_detail(entry)
}

fn load_backups(
    config: &AppState,
    sort_order: BackupSortOrder,
    load_mode: SnapshotLoadMode,
) -> Result<Vec<BackupEntry>, CatalogError> {
    list_backups_in_root(&config.active_backup_path(), sort_order, load_mode)
}

fn list_backups_in_root(
    root: &Path,
    sort_order: BackupSortOrder,
    load_mode: SnapshotLoadMode,
) -> Result<Vec<BackupEntry>, CatalogError> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let entries = fs_ops::read_dir(root, |path, source| CatalogError::ReadCatalog {
        path,
        source,
    })?;

    let mut backups = Vec::new();
    for entry in entries {
        let entry = fs_ops::dir_entry(entry, root, |path, source| CatalogError::ReadCatalog {
            path,
            source,
        })?;
        if let Some(backup) = read_catalog_entry(entry, load_mode)? {
            backups.push(backup);
        }
    }

    sort_backups(&mut backups, sort_order);
    Ok(backups)
}

fn read_backup_entry_in_root(root: &Path, backup_id: &str) -> Result<BackupEntry, CatalogError> {
    let path = root.join(backup_id);
    let metadata = fs_ops::metadata(&path, |path, source| match source.kind() {
        io::ErrorKind::NotFound => missing_backup_name(root, backup_id),
        _ => CatalogError::ReadMetadata { path, source },
    })?;

    if !metadata.is_dir() {
        return Err(missing_backup_name(root, backup_id));
    }

    read_backup_entry(
        path,
        backup_id.to_string(),
        metadata,
        SnapshotLoadMode::Full,
    )
}

fn read_backup_entry(
    path: PathBuf,
    backup_id: String,
    metadata: std::fs::Metadata,
    load_mode: SnapshotLoadMode,
) -> Result<BackupEntry, CatalogError> {
    let snapshot = read_snapshot_for_entry(&path, load_mode)?;

    let modified_at = metadata
        .modified()
        .map_err(|source| CatalogError::ReadMetadata {
            path: path.clone(),
            source,
        })?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    Ok(BackupEntry {
        id: backup_id,
        path,
        modified_at,
        snapshot,
    })
}

fn read_catalog_entry(
    entry: std::fs::DirEntry,
    load_mode: SnapshotLoadMode,
) -> Result<Option<BackupEntry>, CatalogError> {
    let path = entry.path();
    let metadata = entry
        .metadata()
        .map_err(|source| CatalogError::ReadMetadata {
            path: path.clone(),
            source,
        })?;
    if !metadata.is_dir() {
        return Ok(None);
    }

    let backup_id = entry.file_name().to_string_lossy().into_owned();
    read_backup_entry(path, backup_id, metadata, load_mode).map(Some)
}

fn sort_backups(backups: &mut [BackupEntry], sort_order: BackupSortOrder) {
    match sort_order {
        BackupSortOrder::ModifiedAtDesc => backups.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then_with(|| right.id.cmp(&left.id))
        }),
        BackupSortOrder::BackupIdDesc => backups.sort_by(|left, right| right.id.cmp(&left.id)),
    }
}

fn missing_backup_name(root: &Path, backup_id: &str) -> CatalogError {
    CatalogError::MissingBackupName {
        name: backup_id.to_string(),
        root: root.to_path_buf(),
    }
}

fn read_snapshot_for_entry(path: &Path, load_mode: SnapshotLoadMode) -> Result<Tmux, CatalogError> {
    match load_mode {
        SnapshotLoadMode::Full => snapshot::read_snapshot_dir(path).map(|loaded| loaded.tmux),
        SnapshotLoadMode::Summary => snapshot::read_snapshot_summary_dir(path),
    }
    .map_err(|source| CatalogError::ReadSnapshot {
        path: path.to_path_buf(),
        source,
    })
}
