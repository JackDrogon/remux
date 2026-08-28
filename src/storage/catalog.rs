//! Catalog views over persisted backup directories.
//!
//! Listing, lookup, and deletion live together so every CLI path applies the
//! same root-isolation and snapshot-decoding rules. The sort order is
//! intentionally explicit because the interactive list and the default restore
//! target use different stability requirements.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use super::backup_name::normalize_backup_name;
use super::fs_ops;
use super::snapshot;
use crate::config::AppState;
use crate::model::Tmux;
use crate::{Catalog as CatalogError, Result};

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
    pub backup_id: String,
    pub path: PathBuf,
    pub modified_at: Duration,
    pub snapshot: Tmux,
}

pub fn list_backups(config: &AppState) -> Result<Vec<BackupEntry>> {
    tracing::debug!(root = %config.active_backup_path().display(), "listing full backups");
    load_backups(
        config,
        BackupSortOrder::ModifiedAtDesc,
        SnapshotLoadMode::Full,
    )
}

pub fn list_backups_for_listing(config: &AppState) -> Result<Vec<BackupEntry>> {
    tracing::debug!(root = %config.active_backup_path().display(), "listing summary backups");
    load_backups(
        config,
        BackupSortOrder::BackupIdDesc,
        SnapshotLoadMode::Summary,
    )
}

pub fn load_backup(config: &AppState, backup_name: &str) -> Result<BackupEntry> {
    let normalized_name = normalize_backup_name(backup_name)?;
    let root = config.active_backup_path();
    tracing::info!(backup_name = %normalized_name, root = %root.display(), "loading backup entry");
    read_backup_entry_in_root(&root, &normalized_name)
}

pub fn latest_backup(config: &AppState) -> Result<BackupEntry> {
    let root = config.active_backup_path();
    Ok(list_backups(config)?
        .into_iter()
        .next()
        .ok_or(CatalogError::NoBackups { root })?)
}

pub fn load_newest_backups(config: &AppState, limit: usize) -> Result<Vec<BackupEntry>> {
    let root = config.active_backup_path();
    tracing::debug!(
        root = %root.display(),
        limit,
        "loading newest backup directories"
    );
    let mut directories = list_backup_directories(&root)?;
    sort_backup_directories(&mut directories);
    directories.truncate(limit);

    directories
        .into_iter()
        .map(|directory| {
            read_backup_entry(
                directory.path,
                directory.backup_id,
                directory.metadata,
                SnapshotLoadMode::Full,
            )
        })
        .collect()
}

pub fn resolve_restore_target(config: &AppState, requested_name: Option<&str>) -> Result<String> {
    resolve_restore_target_in_root(&config.active_backup_path(), requested_name)
}

pub fn resolve_restore_target_in_root(root: &Path, requested_name: Option<&str>) -> Result<String> {
    match requested_name {
        Some(requested_name) => {
            let normalized = normalize_backup_name(requested_name)?;
            let path = root.join(&normalized);
            let metadata = fs_ops::optional_metadata(&path, |path, source| {
                CatalogError::ReadMetadata { path, source }
            })?;
            match metadata {
                Some(metadata) if metadata.is_dir() => Ok(normalized),
                _ => Err(missing_backup_name(root, &normalized).into()),
            }
        }
        None => {
            let mut directories = list_backup_directories(root)?;
            sort_backup_directories(&mut directories);
            Ok(directories
                .into_iter()
                .next()
                .map(|directory| directory.backup_id)
                .ok_or_else(|| CatalogError::NoBackups {
                    root: root.to_path_buf(),
                })?)
        }
    }
}

pub fn delete_backup(config: &AppState, backup_name: &str) -> Result<()> {
    let entry = load_backup(config, backup_name)?;
    tracing::info!(backup_name = %entry.backup_id, path = %entry.path.display(), "deleting backup entry");
    fs_ops::remove_dir_all(&entry.path, |path, source| CatalogError::DeleteBackup {
        path,
        source,
    })?;
    Ok(())
}

fn load_backups(
    config: &AppState,
    sort_order: BackupSortOrder,
    load_mode: SnapshotLoadMode,
) -> Result<Vec<BackupEntry>> {
    list_backups_in_root(&config.active_backup_path(), sort_order, load_mode)
}

struct BackupDirectory {
    path: PathBuf,
    backup_id: String,
    metadata: std::fs::Metadata,
}

fn list_backup_directories(root: &Path) -> Result<Vec<BackupDirectory>> {
    let Some(entries) = open_catalog_root(root)? else {
        return Ok(Vec::new());
    };

    let mut directories = Vec::new();
    for entry in entries {
        let entry = fs_ops::dir_entry(entry, root, |path, source| CatalogError::ReadCatalog {
            path,
            source,
        })?;
        if let Some(directory) = listed_backup_directory(entry)? {
            directories.push(directory);
        }
    }
    Ok(directories)
}

fn sort_backup_directories(directories: &mut [BackupDirectory]) {
    directories.sort_by(|left, right| {
        directory_modified_at(&right.metadata)
            .cmp(&directory_modified_at(&left.metadata))
            .then_with(|| right.backup_id.cmp(&left.backup_id))
    });
}

fn directory_modified_at(metadata: &std::fs::Metadata) -> Duration {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .unwrap_or_default()
}

fn list_backups_in_root(
    root: &Path,
    sort_order: BackupSortOrder,
    load_mode: SnapshotLoadMode,
) -> Result<Vec<BackupEntry>> {
    let Some(entries) = open_catalog_root(root)? else {
        return Ok(Vec::new());
    };

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

fn read_backup_entry_in_root(root: &Path, backup_id: &str) -> Result<BackupEntry> {
    let path = root.join(backup_id);
    let metadata = fs_ops::metadata(&path, |path, source| match source.kind() {
        io::ErrorKind::NotFound => missing_backup_name(root, backup_id),
        _ => CatalogError::ReadMetadata { path, source },
    })?;

    if !metadata.is_dir() {
        return Err(missing_backup_name(root, backup_id).into());
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
) -> Result<BackupEntry> {
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
        backup_id,
        path,
        modified_at,
        snapshot,
    })
}

fn read_catalog_entry(
    entry: std::fs::DirEntry,
    load_mode: SnapshotLoadMode,
) -> Result<Option<BackupEntry>> {
    let Some(directory) = listed_backup_directory(entry)? else {
        return Ok(None);
    };
    read_backup_entry(
        directory.path,
        directory.backup_id,
        directory.metadata,
        load_mode,
    )
    .map(Some)
}

fn listed_backup_directory(entry: std::fs::DirEntry) -> Result<Option<BackupDirectory>> {
    let backup_id = entry.file_name().to_string_lossy().into_owned();
    if !is_listed_backup_name(&backup_id) {
        return Ok(None);
    }

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

    Ok(Some(BackupDirectory {
        path,
        backup_id,
        metadata,
    }))
}

fn is_listed_backup_name(backup_id: &str) -> bool {
    !backup_id.starts_with('.')
}

/// Whether `path` already occupies a backup-id slot. A dangling symlink counts.
pub(crate) fn backup_id_slot_is_occupied(path: &Path) -> Result<bool> {
    match fs_ops::optional_symlink_metadata(path, |path, source| CatalogError::ReadMetadata {
        path,
        source,
    })? {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogRoot {
    Missing,
    Directory,
    NotDirectory,
}

fn observe_catalog_root(root: &Path) -> Result<CatalogRoot> {
    let Some(entry) = fs_ops::optional_symlink_metadata(root, |path, source| {
        CatalogError::ReadCatalog { path, source }
    })?
    else {
        return Ok(CatalogRoot::Missing);
    };

    if entry.file_type().is_symlink() {
        return match fs_ops::optional_metadata(root, |path, source| CatalogError::ReadCatalog {
            path,
            source,
        })? {
            Some(target) if target.is_dir() => Ok(CatalogRoot::Directory),
            Some(_) | None => Ok(CatalogRoot::NotDirectory),
        };
    }

    if entry.is_dir() {
        Ok(CatalogRoot::Directory)
    } else {
        Ok(CatalogRoot::NotDirectory)
    }
}

fn open_catalog_root(root: &Path) -> Result<Option<std::fs::ReadDir>> {
    match observe_catalog_root(root)? {
        CatalogRoot::Missing => Ok(None),
        CatalogRoot::NotDirectory => Err(CatalogError::RootNotDirectory {
            path: root.to_path_buf(),
        }
        .into()),
        CatalogRoot::Directory => {
            // A race after this observation stays ReadCatalog.
            Ok(Some(fs_ops::read_dir(root, |path, source| {
                CatalogError::ReadCatalog { path, source }
            })?))
        }
    }
}

fn sort_backups(backups: &mut [BackupEntry], sort_order: BackupSortOrder) {
    match sort_order {
        BackupSortOrder::ModifiedAtDesc => backups.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then_with(|| right.backup_id.cmp(&left.backup_id))
        }),
        BackupSortOrder::BackupIdDesc => {
            backups.sort_by(|left, right| right.backup_id.cmp(&left.backup_id))
        }
    }
}

fn missing_backup_name(root: &Path, backup_id: &str) -> CatalogError {
    CatalogError::MissingBackupName {
        name: backup_id.to_string(),
        root: root.to_path_buf(),
    }
}

fn read_snapshot_for_entry(path: &Path, load_mode: SnapshotLoadMode) -> Result<Tmux> {
    let directory = snapshot::SnapshotDirectory::new(path);
    match load_mode {
        SnapshotLoadMode::Full => Ok(directory.read_full()?.tmux),
        SnapshotLoadMode::Summary => directory.read_summary(),
    }
}

#[cfg(test)]
mod occupancy_tests {
    use super::*;
    use crate::Code;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-catalog-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn catalog_root_three_states() {
        let base = scratch("root");
        let missing = base.join("missing");
        assert_eq!(
            observe_catalog_root(&missing).expect("missing root"),
            CatalogRoot::Missing
        );

        let dir = base.join("dir");
        fs::create_dir(&dir).expect("dir");
        assert_eq!(
            observe_catalog_root(&dir).expect("directory root"),
            CatalogRoot::Directory
        );

        let file = base.join("file");
        fs::write(&file, b"x").expect("file");
        assert_eq!(
            observe_catalog_root(&file).expect("file root"),
            CatalogRoot::NotDirectory
        );

        let dangling = base.join("dangling");
        std::os::unix::fs::symlink(base.join("nope"), &dangling).expect("dangling");
        assert_eq!(
            observe_catalog_root(&dangling).expect("dangling root"),
            CatalogRoot::NotDirectory
        );

        let link_dir = base.join("link-dir");
        std::os::unix::fs::symlink(&dir, &link_dir).expect("symlink to dir");
        assert_eq!(
            observe_catalog_root(&link_dir).expect("symlink-to-dir root"),
            CatalogRoot::Directory
        );

        let link_file = base.join("link-file");
        std::os::unix::fs::symlink(&file, &link_file).expect("symlink to file");
        assert_eq!(
            observe_catalog_root(&link_file).expect("symlink-to-file root"),
            CatalogRoot::NotDirectory
        );

        let err = open_catalog_root(&file).expect_err("file root is not listable");
        assert_eq!(err.category(), crate::Category::Catalog);
        assert!(matches!(
            err.code(),
            Code::Catalog(CatalogError::RootNotDirectory { .. })
        ));

        cleanup(&base);
    }

    #[test]
    fn backup_id_slot_lstat_occupancy() {
        let base = scratch("slot");
        let missing = base.join("free");
        assert!(
            !backup_id_slot_is_occupied(&missing).expect("free slot"),
            "NotFound is free"
        );

        let dir = base.join("dir");
        fs::create_dir(&dir).expect("dir");
        assert!(backup_id_slot_is_occupied(&dir).expect("dir occupies"));

        let file = base.join("file");
        fs::write(&file, b"x").expect("file");
        assert!(backup_id_slot_is_occupied(&file).expect("file occupies"));

        let dangling = base.join("dangling");
        std::os::unix::fs::symlink(base.join("nope"), &dangling).expect("dangling");
        assert!(backup_id_slot_is_occupied(&dangling).expect("dangling occupies"));

        let link = base.join("link");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");
        assert!(backup_id_slot_is_occupied(&link).expect("symlink occupies"));

        cleanup(&base);
    }
}
