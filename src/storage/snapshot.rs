//! Persist and validate the on-disk snapshot contract.
//!
//! The snapshot format is intentionally split into a lightweight summary file,
//! a full manifest, and pane content blobs. This keeps list operations cheap
//! while still allowing restore to validate integrity before mutating tmux. The
//! writer uses a temporary directory plus rename so readers never observe a
//! partially written snapshot tree.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::hash::sha256_hex;
use crate::model::Tmux;

use super::fs_ops;
use super::snapshot_contract::{
    build_loaded_snapshot, current_version, ensure_supported_version, read_json_slice,
    summary_count, summary_to_tmux, PaneContentMeta, PaneEncoding, SnapshotManifestFile,
    SnapshotPane, SnapshotSession, SnapshotSize, SnapshotSummaryFile, SnapshotWindow,
};

pub const SUMMARY_FILE_NAME: &str = "summary.json";
pub const MANIFEST_FILE_NAME: &str = "manifest.json";
const PANES_DIR_NAME: &str = "panes";

#[derive(Debug)]
pub enum SnapshotError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    UnsupportedVersion {
        path: PathBuf,
        found_major: u16,
    },
    InvalidSummary {
        path: PathBuf,
        detail: String,
    },
    InvalidManifest {
        path: PathBuf,
        detail: String,
    },
    SummaryManifestMismatch {
        summary_path: PathBuf,
        manifest_path: PathBuf,
        detail: String,
    },
    MissingPaneBytes {
        pane_id: String,
    },
    DuplicatePaneId {
        pane_id: String,
    },
    DuplicateContentRef {
        content_ref: String,
    },
    InvalidRelativePath {
        relative_path: String,
    },
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "I/O error at {}: {source}", path.display()),
            Self::Json { path, source } => {
                write!(f, "JSON error at {}: {source}", path.display())
            }
            Self::UnsupportedVersion { path, found_major } => write!(
                f,
                "unsupported snapshot schema major version {found_major} at {}",
                path.display()
            ),
            Self::InvalidSummary { path, detail } => {
                write!(f, "invalid snapshot summary {}: {detail}", path.display())
            }
            Self::InvalidManifest { path, detail } => {
                write!(f, "invalid snapshot manifest {}: {detail}", path.display())
            }
            Self::SummaryManifestMismatch {
                summary_path,
                manifest_path,
                detail,
            } => write!(
                f,
                "summary/manifest mismatch between {} and {}: {detail}",
                summary_path.display(),
                manifest_path.display()
            ),
            Self::MissingPaneBytes { pane_id } => {
                write!(f, "missing captured pane bytes for {pane_id}")
            }
            Self::DuplicatePaneId { pane_id } => {
                write!(f, "duplicate pane id in snapshot model: {pane_id}")
            }
            Self::DuplicateContentRef { content_ref } => {
                write!(
                    f,
                    "duplicate content_ref in snapshot manifest: {content_ref}"
                )
            }
            Self::InvalidRelativePath { relative_path } => {
                write!(f, "invalid relative snapshot path: {relative_path}")
            }
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::UnsupportedVersion { .. }
            | Self::InvalidSummary { .. }
            | Self::InvalidManifest { .. }
            | Self::SummaryManifestMismatch { .. }
            | Self::MissingPaneBytes { .. }
            | Self::DuplicatePaneId { .. }
            | Self::DuplicateContentRef { .. }
            | Self::InvalidRelativePath { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneAsset {
    pub relative_path: PathBuf,
    pub byte_len: u64,
    pub sha256: String,
    pub encoding: PaneEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSnapshot {
    pub tmux: Tmux,
    pub pane_assets: BTreeMap<String, PaneAsset>,
}

pub fn write_snapshot_dir(
    snapshot_dir: &Path,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<(), SnapshotError> {
    let parent = snapshot_parent_dir(snapshot_dir)?;
    create_dir_all(parent)?;

    let temp_dir = prepare_temp_dir(snapshot_dir)?;
    let panes_dir = write_snapshot_payload(&temp_dir, tmux, pane_contents)?;

    sync_dir(&panes_dir)?;
    sync_dir(&temp_dir)?;
    fs_ops::rename(&temp_dir, snapshot_dir, io_error)?;
    sync_dir(parent)?;
    Ok(())
}

pub fn read_snapshot_summary_dir(snapshot_dir: &Path) -> Result<Tmux, SnapshotError> {
    let summary_path = snapshot_dir.join(SUMMARY_FILE_NAME);
    let summary = read_summary_file(&summary_path)?;
    Ok(summary_to_tmux(&summary))
}

pub fn read_snapshot_dir(snapshot_dir: &Path) -> Result<LoadedSnapshot, SnapshotError> {
    let summary_path = snapshot_dir.join(SUMMARY_FILE_NAME);
    let manifest_path = snapshot_dir.join(MANIFEST_FILE_NAME);

    let summary = read_summary_file(&summary_path)?;
    let manifest_bytes = read_file(&manifest_path)?;
    let actual_hash = sha256_hex(&manifest_bytes);
    if summary.manifest_sha256 != actual_hash {
        return Err(summary_manifest_mismatch(
            summary_path,
            manifest_path.clone(),
            format!(
                "expected manifest sha {}, found {}",
                summary.manifest_sha256, actual_hash
            ),
        ));
    }

    let manifest = parse_manifest_file(&manifest_path, &manifest_bytes)?;
    validate_summary_matches_manifest(&summary, &manifest, &manifest_path)?;
    let (tmux, pane_assets) = build_loaded_snapshot(snapshot_dir, manifest)?;
    Ok(LoadedSnapshot { tmux, pane_assets })
}

fn snapshot_parent_dir(snapshot_dir: &Path) -> Result<&Path, SnapshotError> {
    snapshot_dir
        .parent()
        .ok_or_else(|| SnapshotError::InvalidManifest {
            path: snapshot_dir.to_path_buf(),
            detail: "snapshot directory must have a parent".to_string(),
        })
}

fn write_snapshot_payload(
    temp_dir: &Path,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<PathBuf, SnapshotError> {
    let panes_dir = temp_dir.join(PANES_DIR_NAME);
    create_dir_all(&panes_dir)?;

    let manifest = build_manifest(tmux, pane_contents)?;
    write_pane_files(temp_dir, pane_contents, &manifest.pane_table)?;

    let manifest_path = temp_dir.join(MANIFEST_FILE_NAME);
    let manifest_bytes = write_json_file(&manifest_path, &manifest)?;
    let summary = build_summary(tmux, &manifest, &manifest_bytes)?;
    let summary_path = temp_dir.join(SUMMARY_FILE_NAME);
    write_json_file(&summary_path, &summary)?;

    Ok(panes_dir)
}

fn prepare_temp_dir(snapshot_dir: &Path) -> Result<PathBuf, SnapshotError> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let snapshot_name = snapshot_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| SnapshotError::InvalidManifest {
            path: snapshot_dir.to_path_buf(),
            detail: "snapshot directory name must be valid UTF-8".to_string(),
        })?;
    let temp_name = format!(".{snapshot_name}.tmp-{}-{stamp}", std::process::id());
    let temp_dir = snapshot_dir.with_file_name(temp_name);
    if temp_dir.exists() {
        fs_ops::remove_dir_all(&temp_dir, io_error)?;
    }
    create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}

fn build_manifest(
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<SnapshotManifestFile, SnapshotError> {
    let mut sessions = Vec::new();
    let mut pane_table = BTreeMap::new();
    let mut seen_pane_ids = BTreeMap::new();

    for session in &tmux.sessions {
        let mut windows = Vec::new();
        for window in &session.windows {
            let mut panes = Vec::new();
            for pane in &window.panes {
                let pane_id = pane.idstr();
                if seen_pane_ids.insert(pane_id.clone(), ()).is_some() {
                    return Err(SnapshotError::DuplicatePaneId { pane_id });
                }

                let content_ref = pane_id.clone();
                if pane_table.contains_key(&content_ref) {
                    return Err(SnapshotError::DuplicateContentRef { content_ref });
                }

                let content =
                    pane_contents
                        .get(&pane_id)
                        .ok_or_else(|| SnapshotError::MissingPaneBytes {
                            pane_id: pane_id.clone(),
                        })?;
                let encoding = if std::str::from_utf8(content).is_ok() {
                    PaneEncoding::Utf8
                } else {
                    PaneEncoding::Binary
                };
                let extension = match encoding {
                    PaneEncoding::Utf8 => "txt",
                    PaneEncoding::Binary => "bin",
                };
                let relative_path = format!("{PANES_DIR_NAME}/{content_ref}.{extension}");
                validate_relative_path(&relative_path)?;

                pane_table.insert(
                    content_ref.clone(),
                    PaneContentMeta {
                        relative_path,
                        encoding,
                        byte_len: content.len() as u64,
                        sha256: sha256_hex(content),
                    },
                );
                panes.push(SnapshotPane {
                    pane_id: pane.pane_id,
                    active: pane.active,
                    path: pane.path.clone(),
                    size: SnapshotSize::from_size(pane.size),
                    content_ref,
                });
            }

            windows.push(SnapshotWindow {
                id: window.win_id,
                name: window.name.clone(),
                active: window.active,
                layout: window.layout.clone(),
                panes,
            });
        }

        sessions.push(SnapshotSession {
            name: session.name.clone(),
            attached: session.attached,
            size: SnapshotSize::from_size(session.size),
            windows,
        });
    }

    Ok(SnapshotManifestFile {
        schema_version: current_version(),
        backup_id: tmux.tid.clone(),
        created_at: tmux.create_time.clone(),
        sessions,
        pane_table,
    })
}

fn build_summary(
    tmux: &Tmux,
    manifest: &SnapshotManifestFile,
    manifest_bytes: &[u8],
) -> Result<SnapshotSummaryFile, SnapshotError> {
    let session_count = tmux.sessions.len();
    let window_count = tmux
        .sessions
        .iter()
        .map(|session| session.windows.len())
        .sum::<usize>();
    let pane_count = tmux
        .sessions
        .iter()
        .flat_map(|session| session.windows.iter())
        .map(|window| window.panes.len())
        .sum::<usize>();

    Ok(SnapshotSummaryFile {
        schema_version: current_version(),
        backup_id: manifest.backup_id.clone(),
        created_at: manifest.created_at.clone(),
        session_count: summary_count(session_count, "session_count")?,
        window_count: summary_count(window_count, "window_count")?,
        pane_count: summary_count(pane_count, "pane_count")?,
        session_names: tmux
            .sessions
            .iter()
            .map(|session| session.name.clone())
            .collect(),
        active_session: tmux
            .sessions
            .iter()
            .find(|session| session.attached)
            .map(|session| session.name.clone())
            .or_else(|| tmux.sessions.first().map(|session| session.name.clone())),
        manifest_sha256: sha256_hex(manifest_bytes),
    })
}

fn write_pane_files(
    snapshot_dir: &Path,
    pane_contents: &BTreeMap<String, Vec<u8>>,
    pane_table: &BTreeMap<String, PaneContentMeta>,
) -> Result<(), SnapshotError> {
    for (content_ref, meta) in pane_table {
        let pane_bytes =
            pane_contents
                .get(content_ref)
                .ok_or_else(|| SnapshotError::MissingPaneBytes {
                    pane_id: content_ref.clone(),
                })?;
        let pane_path = snapshot_dir.join(validated_relative_path(&meta.relative_path)?);
        if let Some(parent) = pane_path.parent() {
            create_dir_all(parent)?;
        }
        fs_ops::write_bytes(&pane_path, pane_bytes, io_error)?;
        sync_file(&pane_path)?;
    }
    Ok(())
}

fn write_json_file<T>(path: &Path, value: &T) -> Result<Vec<u8>, SnapshotError>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| SnapshotError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    fs_ops::write_bytes(path, &bytes, io_error)?;
    sync_file(path)?;
    Ok(bytes)
}

fn read_summary_file(path: &Path) -> Result<SnapshotSummaryFile, SnapshotError> {
    let bytes = read_file(path)?;
    let summary = read_json_slice::<SnapshotSummaryFile>(path, &bytes)?;
    ensure_supported_version(summary.schema_version, path)?;
    if summary.session_count as usize != summary.session_names.len() {
        return Err(SnapshotError::InvalidSummary {
            path: path.to_path_buf(),
            detail: "session_count does not match session_names length".to_string(),
        });
    }
    Ok(summary)
}

fn parse_manifest_file(path: &Path, bytes: &[u8]) -> Result<SnapshotManifestFile, SnapshotError> {
    let manifest = read_json_slice::<SnapshotManifestFile>(path, bytes)?;
    ensure_supported_version(manifest.schema_version, path)?;
    Ok(manifest)
}

fn validate_summary_matches_manifest(
    summary: &SnapshotSummaryFile,
    manifest: &SnapshotManifestFile,
    manifest_path: &Path,
) -> Result<(), SnapshotError> {
    if summary.backup_id != manifest.backup_id {
        return Err(summary_manifest_mismatch(
            PathBuf::from(SUMMARY_FILE_NAME),
            manifest_path.to_path_buf(),
            "backup_id differs between summary and manifest",
        ));
    }
    if summary.created_at != manifest.created_at {
        return Err(summary_manifest_mismatch(
            PathBuf::from(SUMMARY_FILE_NAME),
            manifest_path.to_path_buf(),
            "created_at differs between summary and manifest",
        ));
    }
    Ok(())
}

fn summary_manifest_mismatch(
    summary_path: PathBuf,
    manifest_path: PathBuf,
    detail: impl Into<String>,
) -> SnapshotError {
    SnapshotError::SummaryManifestMismatch {
        summary_path,
        manifest_path,
        detail: detail.into(),
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, SnapshotError> {
    fs_ops::read_bytes(path, io_error)
}

fn create_dir_all(path: &Path) -> Result<(), SnapshotError> {
    fs_ops::create_dir_all(path, io_error)
}

fn sync_file(path: &Path) -> Result<(), SnapshotError> {
    fs_ops::sync_file(path, io_error)
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), SnapshotError> {
    fs_ops::sync_dir(path, io_error)
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), SnapshotError> {
    Ok(())
}

fn validate_relative_path(relative_path: &str) -> Result<(), SnapshotError> {
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty() {
        return Err(SnapshotError::InvalidRelativePath {
            relative_path: relative_path.to_string(),
        });
    }

    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(SnapshotError::InvalidRelativePath {
                    relative_path: relative_path.to_string(),
                });
            }
        }
    }

    Ok(())
}

pub(crate) fn validated_relative_path(relative_path: &str) -> Result<PathBuf, SnapshotError> {
    validate_relative_path(relative_path)?;
    Ok(PathBuf::from(relative_path))
}

fn io_error(path: PathBuf, source: io::Error) -> SnapshotError {
    SnapshotError::Io { path, source }
}
