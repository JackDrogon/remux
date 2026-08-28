use crate::config::AppState;
use crate::error::Result;
use crate::storage::{
    self, CompactFingerprint, SnapshotDirectory, delete_backup, is_automatic_backup_id,
    load_newest_backups,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactOutcome {
    NeedMoreBackups,
    NamedPrevious { name: String },
    Different { kept: String, previous: String },
    Removed { deleted: String, kept: String },
}

pub fn compact_latest_pair(config: &AppState) -> Result<CompactOutcome> {
    let backups = load_newest_backups(config, 2)?;
    let Some((newer, older)) = backups.first().zip(backups.get(1)) else {
        tracing::info!(
            backup_count = backups.len(),
            "compact finished: need at least two backups"
        );
        return Ok(CompactOutcome::NeedMoreBackups);
    };

    if !is_automatic_backup_id(&older.backup_id) {
        tracing::info!(
            kept = %newer.backup_id,
            previous = %older.backup_id,
            "compact finished: previous is not an automatic backup"
        );
        return Ok(CompactOutcome::NamedPrevious {
            name: older.backup_id.clone(),
        });
    }

    let newer_fingerprint = fingerprint_for_readable_entry(newer)?;
    let older_fingerprint = fingerprint_for_readable_entry(older)?;
    if newer_fingerprint != older_fingerprint {
        tracing::info!(
            kept = %newer.backup_id,
            previous = %older.backup_id,
            "compact finished: latest backups differ"
        );
        return Ok(CompactOutcome::Different {
            kept: newer.backup_id.clone(),
            previous: older.backup_id.clone(),
        });
    }

    let deleted = older.backup_id.clone();
    let kept = newer.backup_id.clone();
    delete_backup(config, &deleted)?;
    tracing::info!(
        kept = %kept,
        removed = %deleted,
        "compact finished: removed duplicate backup"
    );
    Ok(CompactOutcome::Removed { deleted, kept })
}

fn fingerprint_for_readable_entry(entry: &storage::BackupEntry) -> Result<CompactFingerprint> {
    let directory = SnapshotDirectory::new(&entry.path);
    let loaded = directory.read_full()?;
    directory.validate_all_assets(&loaded.pane_assets)?;
    let (schema_major, schema_minor) = directory.schema_version()?;
    Ok(CompactFingerprint::from_tmux(
        &loaded.tmux,
        schema_major,
        schema_minor,
    ))
}
