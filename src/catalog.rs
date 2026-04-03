use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::backup_name::{BackupNameError, normalize_backup_name};
use crate::config::AppState;
use crate::model::Tmux;
use crate::serde_legacy::{self, LegacySnapshotError};

const LIST_WIDTH: usize = 72;
const TREE_SPACE: &str = "        ";

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

pub fn list_backups(config: &AppState) -> Result<Vec<BackupEntry>, CatalogError> {
    list_backups_in_root(
        &config.active_backup_path(),
        BackupSortOrder::ModifiedAtDesc,
        SnapshotLoadMode::Full,
    )
}

pub fn list_backups_for_listing(config: &AppState) -> Result<Vec<BackupEntry>, CatalogError> {
    list_backups_in_root(
        &config.active_backup_path(),
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
    fs::remove_dir_all(&entry.path).map_err(|source| CatalogError::DeleteBackup {
        path: entry.path,
        source,
    })
}

pub fn no_backups_message() -> &'static str {
    "No backup was created yet.\nremux -b [name] to create backup"
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
    format!(
        "Details of backup:{}\n{}\n{}",
        entry.id,
        repeat_line('='),
        render_detail_body(&entry.snapshot)
    )
}

pub fn render_interactive_detail(entry: &BackupEntry) -> String {
    format!(
        "{}\nDetails of backup:{}\n{}\n{}\n{}",
        repeat_line('>'),
        entry.id,
        repeat_line('>'),
        render_detail_body(&entry.snapshot),
        repeat_line('<')
    )
}

fn render_detail_body(tmux: &Tmux) -> String {
    let mut lines = vec![format!(
        "{:>LIST_WIDTH$}",
        format!("Backup was created on {}", tmux.create_time)
    )];
    lines.push(format!(
        " Backup─┬─[{}] ({} sessions):",
        tmux.tid,
        tmux.sessions.len()
    ));

    for (session_index, session) in tmux.sessions.iter().enumerate() {
        let is_last_session = session_index + 1 == tmux.sessions.len();
        let session_text = format!(
            "─Session─┬─[{}] ({} windows):",
            session.name,
            session.windows.len()
        );
        lines.push(tree_struc(session_text, &[is_last_session], 1, false));

        for (window_index, window) in session.windows.iter().enumerate() {
            let is_last_window = window_index + 1 == session.windows.len();
            let window_text = format!(
                "─Window─┬─({}) [{}] ({} panes):",
                window.win_id,
                window.name,
                window.panes.len()
            );
            lines.push(tree_struc(
                window_text,
                &[is_last_session, is_last_window],
                2,
                false,
            ));

            for (pane_index, pane) in window.panes.iter().enumerate() {
                let is_last_pane = pane_index + 1 == window.panes.len();
                let pane_text = format!("─Pane ({}) {}", pane.pane_id, pane.path);
                lines.push(tree_struc(
                    pane_text,
                    &[is_last_session, is_last_window, is_last_pane],
                    3,
                    false,
                ));
            }
        }
    }

    lines.join("\n")
}

fn list_backups_in_root(
    root: &Path,
    sort_order: BackupSortOrder,
    load_mode: SnapshotLoadMode,
) -> Result<Vec<BackupEntry>, CatalogError> {
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
        backups.push(read_backup_entry(path, backup_id, metadata, load_mode)?);
    }

    match sort_order {
        BackupSortOrder::ModifiedAtDesc => backups.sort_by(|left, right| {
            right
                .modified_at
                .cmp(&left.modified_at)
                .then_with(|| right.id.cmp(&left.id))
        }),
        BackupSortOrder::BackupIdDesc => backups.sort_by(|left, right| right.id.cmp(&left.id)),
    }

    Ok(backups)
}

fn read_backup_entry_in_root(root: &Path, backup_id: &str) -> Result<BackupEntry, CatalogError> {
    let path = root.join(backup_id);
    let metadata = fs::metadata(&path).map_err(|source| match source.kind() {
        io::ErrorKind::NotFound => CatalogError::MissingBackupName {
            name: backup_id.to_string(),
            root: root.to_path_buf(),
        },
        _ => CatalogError::ReadMetadata {
            path: path.clone(),
            source,
        },
    })?;

    if !metadata.is_dir() {
        return Err(CatalogError::MissingBackupName {
            name: backup_id.to_string(),
            root: root.to_path_buf(),
        });
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
    metadata: fs::Metadata,
    load_mode: SnapshotLoadMode,
) -> Result<BackupEntry, CatalogError> {
    let snapshot_path = path.join(format!("{backup_id}.json"));
    let snapshot = match load_mode {
        SnapshotLoadMode::Full => serde_legacy::read_snapshot_file(&snapshot_path),
        SnapshotLoadMode::Summary => serde_legacy::read_snapshot_summary_file(&snapshot_path),
    }
    .map_err(|source| CatalogError::ReadSnapshot {
        path: snapshot_path,
        source,
    })?;

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

fn repeat_line(ch: char) -> String {
    ch.to_string().repeat(LIST_WIDTH)
}

fn tree_struc(text: String, is_last_list: &[bool], lvl: usize, place_holder: bool) -> String {
    if lvl == 0 {
        return text;
    }

    let current_level = lvl - 1;
    let mut line = if is_last_list[current_level] {
        let node = if place_holder { ' ' } else { '└' };
        format!("{TREE_SPACE}{node}{text}")
    } else {
        let node = if place_holder { '│' } else { '├' };
        format!("{TREE_SPACE}{node}{text}")
    };

    if current_level == 1 {
        line = format!(" {line}");
    }

    tree_struc(line, is_last_list, current_level, true)
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
