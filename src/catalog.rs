use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::backup_name::{BackupNameError, normalize_backup_name};
use crate::config::RuntimeConfig;
use crate::model::Tmux;
use crate::serde_legacy::{self, LegacySnapshotError};

const LIST_WIDTH: usize = 72;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    pub id: String,
    pub path: PathBuf,
    pub modified_at: Duration,
    pub snapshot: Tmux,
}

#[derive(Debug)]
pub enum CatalogError {
    InvalidBackupName(BackupNameError),
    ReadCatalog {
        path: PathBuf,
        source: io::Error,
    },
    ReadMetadata {
        path: PathBuf,
        source: io::Error,
    },
    ReadSnapshot {
        path: PathBuf,
        source: LegacySnapshotError,
    },
    MissingBackupName {
        name: String,
        root: PathBuf,
    },
    NoBackups {
        root: PathBuf,
    },
    DeleteBackup {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBackupName(error) => write!(f, "{error}"),
            Self::ReadCatalog { path, source } => {
                write!(
                    f,
                    "failed to read backup catalog {}: {source}",
                    path.display()
                )
            }
            Self::ReadMetadata { path, source } => {
                write!(
                    f,
                    "failed to read backup metadata {}: {source}",
                    path.display()
                )
            }
            Self::ReadSnapshot { path, source } => {
                write!(
                    f,
                    "failed to read backup snapshot {}: {source}",
                    path.display()
                )
            }
            Self::MissingBackupName { name, .. } => {
                write!(f, "cannot find given backup name:{name}")
            }
            Self::NoBackups { root } => write!(
                f,
                "backup dir is empty under {}, nothing to resolve",
                root.display()
            ),
            Self::DeleteBackup { path, source } => {
                write!(f, "failed to delete backup {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidBackupName(error) => Some(error),
            Self::ReadCatalog { source, .. }
            | Self::ReadMetadata { source, .. }
            | Self::DeleteBackup { source, .. } => Some(source),
            Self::ReadSnapshot { source, .. } => Some(source),
            Self::MissingBackupName { .. } | Self::NoBackups { .. } => None,
        }
    }
}

pub fn list_backups(config: &RuntimeConfig) -> Result<Vec<BackupEntry>, CatalogError> {
    list_backups_in_root(config.active_backup_path())
}

pub fn load_backup(config: &RuntimeConfig, backup_name: &str) -> Result<BackupEntry, CatalogError> {
    let normalized_name =
        normalize_backup_name(backup_name).map_err(CatalogError::InvalidBackupName)?;
    let root = config.active_backup_path().to_path_buf();

    list_backups(config)?
        .into_iter()
        .find(|entry| entry.id == normalized_name)
        .ok_or_else(|| CatalogError::MissingBackupName {
            name: normalized_name.clone(),
            root,
        })
}

pub fn latest_backup(config: &RuntimeConfig) -> Result<BackupEntry, CatalogError> {
    let root = config.active_backup_path().to_path_buf();
    list_backups(config)?
        .into_iter()
        .next()
        .ok_or(CatalogError::NoBackups { root })
}

pub fn resolve_restore_target(
    config: &RuntimeConfig,
    requested_name: Option<&str>,
) -> Result<String, CatalogError> {
    match requested_name {
        Some(requested_name) => Ok(load_backup(config, requested_name)?.id),
        _ => Ok(latest_backup(config)?.id),
    }
}

pub fn delete_backup(config: &RuntimeConfig, backup_name: &str) -> Result<(), CatalogError> {
    let entry = load_backup(config, backup_name)?;
    fs::remove_dir_all(&entry.path).map_err(|source| CatalogError::DeleteBackup {
        path: entry.path,
        source,
    })
}

pub fn no_backups_message() -> &'static str {
    "No backup was created yet.\nretmux -b [name] to create backup"
}

pub fn render_summary(backups: &[BackupEntry]) -> String {
    let mut lines = Vec::new();
    lines.push(repeat_line('='));
    lines.push(format!(
        " {:>2} {}",
        "No.",
        format_short_info_columns("Name", "Sessions", "Created on")
    ));
    lines.push(repeat_line('='));

    for (index, entry) in backups.iter().enumerate() {
        let latest_flag = if index == 0 { '*' } else { ' ' };
        lines.push(format!(
            "{latest_flag}{:>2} {}",
            index + 1,
            format_short_info(&entry.snapshot)
        ));
    }

    lines.push(repeat_line('-'));
    lines.push(format!("{:>LIST_WIDTH$}", "Latest default backup with (*)"));
    lines.join("\n")
}

pub fn render_detail(entry: &BackupEntry) -> String {
    let tmux = &entry.snapshot;
    let mut lines = vec![
        format!("Details of backup:{}", entry.id),
        repeat_line('='),
        format!(
            "{:>LIST_WIDTH$}",
            format!("Backup was created on {}", tmux.create_time)
        ),
        format!("Backup [{}] ({} sessions):", tmux.tid, tmux.sessions.len()),
    ];

    for session in &tmux.sessions {
        lines.push(format!(
            "  Session [{}] ({} windows):",
            session.name,
            session.windows.len()
        ));
        for window in &session.windows {
            lines.push(format!(
                "    Window ({}) [{}] ({} panes):",
                window.win_id,
                window.name,
                window.panes.len()
            ));
            for pane in &window.panes {
                lines.push(format!("      Pane ({}) {}", pane.pane_id, pane.path));
            }
        }
    }

    lines.join("\n")
}

fn list_backups_in_root(root: &Path) -> Result<Vec<BackupEntry>, CatalogError> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(root).map_err(|source| CatalogError::ReadCatalog {
        path: root.to_path_buf(),
        source,
    })?;

    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| CatalogError::ReadCatalog {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .map_err(|source| CatalogError::ReadMetadata {
                path: path.clone(),
                source,
            })?;
        if !metadata.is_dir() {
            continue;
        }

        let backup_id = entry.file_name().to_string_lossy().into_owned();
        let snapshot_path = path.join(format!("{backup_id}.json"));
        let snapshot = serde_legacy::read_snapshot_file(&snapshot_path).map_err(|source| {
            CatalogError::ReadSnapshot {
                path: snapshot_path,
                source,
            }
        })?;

        let modified_at = metadata
            .modified()
            .map_err(|source| CatalogError::ReadMetadata {
                path: path.clone(),
                source,
            })?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        backups.push(BackupEntry {
            id: backup_id,
            path,
            modified_at,
            snapshot,
        });
    }

    backups.sort_by(|left, right| {
        right
            .modified_at
            .cmp(&left.modified_at)
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(backups)
}

fn repeat_line(ch: char) -> String {
    ch.to_string().repeat(LIST_WIDTH)
}

fn format_short_info(tmux: &Tmux) -> String {
    format_short_info_columns(
        &tmux.tid,
        &tmux
            .sessions
            .iter()
            .map(|session| session.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        &tmux.create_time,
    )
}

fn format_short_info_columns(name: &str, sessions: &str, created_on: &str) -> String {
    format!("{name:<17} {sessions:<30} {created_on}")
}
