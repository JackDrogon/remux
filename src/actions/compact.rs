use thiserror::Error;

use crate::backup_name::is_automatic_backup_id;
use crate::config::AppState;
use crate::storage::{
    self, CatalogError, SnapshotError, compact::fingerprint, delete_backup, load_newest_backups,
    read_schema_version, read_snapshot_dir, validate_pane_assets,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactOutcome {
    NeedMoreBackups,
    NamedPrevious { name: String },
    Different,
    Removed { deleted: String, kept: String },
}

#[derive(Debug, Error)]
pub enum CompactError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
}

pub fn compact_latest_pair(config: &AppState) -> Result<CompactOutcome, CompactError> {
    let backups = load_newest_backups(config, 2)?;
    let Some((newer, older)) = backups.first().zip(backups.get(1)) else {
        return Ok(CompactOutcome::NeedMoreBackups);
    };

    if !is_automatic_backup_id(&older.backup_id) {
        return Ok(CompactOutcome::NamedPrevious {
            name: older.backup_id.clone(),
        });
    }

    let newer_fingerprint = fingerprint_for_readable_entry(newer)?;
    let older_fingerprint = fingerprint_for_readable_entry(older)?;
    if newer_fingerprint != older_fingerprint {
        return Ok(CompactOutcome::Different);
    }

    let deleted = older.backup_id.clone();
    let kept = newer.backup_id.clone();
    delete_backup(config, &deleted)?;
    Ok(CompactOutcome::Removed { deleted, kept })
}

fn fingerprint_for_readable_entry(
    entry: &storage::BackupEntry,
) -> Result<storage::compact::CompactFingerprint, CompactError> {
    let loaded = read_snapshot_dir(&entry.path)?;
    validate_pane_assets(&entry.path, &loaded.pane_assets)?;
    let (schema_major, schema_minor) = read_schema_version(&entry.path)?;
    Ok(fingerprint(&loaded.tmux, schema_major, schema_minor))
}
