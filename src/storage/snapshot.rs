//! Persist and validate the on-disk snapshot contract.
//!
//! The snapshot format is intentionally split into a lightweight summary file,
//! a full manifest, and pane content blobs. This keeps list operations cheap
//! while still allowing restore to validate integrity before mutating tmux. The
//! writer uses a temporary directory plus rename so readers never observe a
//! partially written snapshot tree.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use sha2::{Digest, Sha256};

use crate::model::Tmux;
use crate::{Error, Result, Snapshot as SnapshotError};
use xerror::Context;

use super::fs_ops;
use super::snapshot_contract::{
    PaneContentMeta, PaneEncoding, SnapshotManifestFile, SnapshotPane, SnapshotSession,
    SnapshotSize, SnapshotSummaryFile, SnapshotWindow, build_loaded_snapshot, current_version,
    ensure_supported_version, read_json_slice, snapshot_from_process, summary_count,
    summary_to_tmux,
};

pub const SUMMARY_FILE_NAME: &str = "summary.json";
pub const MANIFEST_FILE_NAME: &str = "manifest.json";
const PANES_DIR_NAME: &str = "panes";

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

#[derive(Debug, Clone, Copy)]
pub struct SnapshotDirectory<'a> {
    path: &'a Path,
}

impl<'a> SnapshotDirectory<'a> {
    pub fn new(path: &'a Path) -> Self {
        Self { path }
    }

    pub fn read_full(&self) -> Result<LoadedSnapshot> {
        read_snapshot_dir(self.path)
    }

    pub fn read_summary(&self) -> Result<Tmux> {
        read_snapshot_summary_dir(self.path)
    }

    pub fn schema_version(&self) -> Result<(u16, u16)> {
        read_schema_version(self.path)
    }

    pub fn validate_asset(&self, pane_id: &str, asset: &PaneAsset) -> Result<PathBuf> {
        validate_pane_asset(self.path, pane_id, asset)
    }

    pub fn validate_all_assets(&self, pane_assets: &BTreeMap<String, PaneAsset>) -> Result<()> {
        validate_pane_assets(self.path, pane_assets)
    }
}

pub fn write_snapshot_dir(
    snapshot_dir: &Path,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    tracing::info!(
        snapshot_dir = %snapshot_dir.display(),
        session_count = tmux.sessions.len(),
        pane_count = pane_contents.len(),
        "writing snapshot directory"
    );
    let parent = snapshot_parent_dir(snapshot_dir)?;
    create_dir_all(parent)?;

    let temp_dir = prepare_temp_dir(snapshot_dir)?;
    if let Err(error) = write_unpublished_snapshot(&temp_dir, tmux, pane_contents) {
        return Err(discard_unpublished_snapshot(&temp_dir, error));
    }
    if let Err(error) = fs_ops::rename_noreplace(&temp_dir, snapshot_dir, io_error) {
        return Err(discard_unpublished_snapshot(&temp_dir, error));
    }
    // Published: `temp_dir` is no longer a temp. Parent fsync is durability of
    // the catalog, not temp lifetime. The snapshot is already visible.
    sync_dir(parent).with_context(|| {
        format!(
            "snapshot {} was published, but failed to sync catalog directory {}",
            snapshot_dir.display(),
            parent.display()
        )
    })?;
    tracing::info!(snapshot_dir = %snapshot_dir.display(), "snapshot directory committed");
    Ok(())
}

fn write_unpublished_snapshot(
    temp_dir: &Path,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    let panes_dir = write_snapshot_payload(temp_dir, tmux, pane_contents)?;
    sync_dir(&panes_dir)?;
    sync_dir(temp_dir)?;
    Ok(())
}

fn discard_unpublished_snapshot(temp_dir: &Path, error: Error) -> Error {
    match fs_ops::remove_dir_all(temp_dir, io_error) {
        Ok(()) => error,
        Err(cleanup) => crate::error::attach_context(
            error,
            format!(
                "also failed to remove temp snapshot {}: {cleanup}",
                temp_dir.display()
            ),
        ),
    }
}

pub fn read_snapshot_summary_dir(snapshot_dir: &Path) -> Result<Tmux> {
    let summary_path = snapshot_dir.join(SUMMARY_FILE_NAME);
    let summary = read_summary_file(&summary_path)?;
    Ok(summary_to_tmux(&summary))
}

pub fn validate_pane_asset(
    snapshot_dir: &Path,
    pane_id: &str,
    asset: &PaneAsset,
) -> Result<PathBuf> {
    let content_path = snapshot_dir.join(&asset.relative_path);
    let metadata = fs_ops::optional_metadata(&content_path, io_error)?;
    if !metadata.is_some_and(|metadata| metadata.is_file()) {
        return Err(SnapshotError::MissingPaneContent {
            pane_id: pane_id.to_string(),
            path: content_path,
        }
        .into());
    }

    let bytes = read_file(&content_path)?;
    if bytes.len() as u64 != asset.byte_len {
        return Err(SnapshotError::InvalidPaneContent {
            pane_id: pane_id.to_string(),
            path: content_path,
            detail: format!("expected {} bytes, found {}", asset.byte_len, bytes.len()),
        }
        .into());
    }

    let actual_hash = sha256_hex(&bytes);
    if actual_hash != asset.sha256 {
        return Err(SnapshotError::InvalidPaneContent {
            pane_id: pane_id.to_string(),
            path: content_path,
            detail: format!("expected sha256 {}, found {actual_hash}", asset.sha256),
        }
        .into());
    }

    Ok(content_path)
}

pub fn validate_pane_assets(
    snapshot_dir: &Path,
    pane_assets: &BTreeMap<String, PaneAsset>,
) -> Result<()> {
    for (pane_id, asset) in pane_assets {
        validate_pane_asset(snapshot_dir, pane_id, asset)?;
    }
    Ok(())
}

pub fn read_schema_version(snapshot_dir: &Path) -> Result<(u16, u16)> {
    let summary_path = snapshot_dir.join(SUMMARY_FILE_NAME);
    let summary = read_summary_file(&summary_path)?;
    Ok((summary.schema_version.major, summary.schema_version.minor))
}

pub fn read_snapshot_dir(snapshot_dir: &Path) -> Result<LoadedSnapshot> {
    tracing::debug!(snapshot_dir = %snapshot_dir.display(), "reading snapshot directory");
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
    validate_summary_matches_manifest(&summary, &manifest, &summary_path, &manifest_path)?;
    let (tmux, pane_assets) = build_loaded_snapshot(snapshot_dir, manifest)?;
    tracing::debug!(
        snapshot_dir = %snapshot_dir.display(),
        session_count = tmux.sessions.len(),
        pane_asset_count = pane_assets.len(),
        "snapshot directory loaded"
    );
    Ok(LoadedSnapshot { tmux, pane_assets })
}

fn snapshot_parent_dir(snapshot_dir: &Path) -> Result<&Path> {
    snapshot_dir.parent().ok_or_else(|| {
        SnapshotError::InvalidManifest {
            path: snapshot_dir.to_path_buf(),
            detail: "snapshot directory must have a parent".to_string(),
        }
        .into()
    })
}

fn write_snapshot_payload(
    temp_dir: &Path,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<PathBuf> {
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

fn prepare_temp_dir(snapshot_dir: &Path) -> Result<PathBuf> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let snapshot_name = snapshot_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| -> Error {
            SnapshotError::InvalidManifest {
                path: snapshot_dir.to_path_buf(),
                detail: "snapshot directory name must be valid UTF-8".to_string(),
            }
            .into()
        })?;
    let pid = std::process::id();
    // mkdir is exclusive: among cooperating remux instances, success means this
    // call owns the directory. Collision retries; never remove a path we did
    // not create. This is not a dirfd/inode defense against a hostile same-UID
    // process replacing the path.
    for attempt in 0..128u32 {
        let temp_name = format!(".{snapshot_name}.tmp-{pid}-{stamp}-{attempt}");
        let temp_dir = snapshot_dir.with_file_name(temp_name);
        match std::fs::create_dir(&temp_dir) {
            Ok(()) => return Ok(temp_dir),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(io_error(temp_dir, source)),
        }
    }
    Err(SnapshotError::SnapshotIo {
        path: snapshot_dir.to_path_buf(),
        source: io::Error::other("could not allocate an unpublished snapshot directory"),
    }
    .into())
}

fn build_manifest(
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<SnapshotManifestFile> {
    let mut sessions = Vec::new();
    let mut pane_table = BTreeMap::new();
    let mut seen_pane_ids = BTreeMap::new();

    for session in &tmux.sessions {
        let mut windows = Vec::new();
        for window in &session.windows {
            let mut panes = Vec::new();
            for pane in &window.panes {
                let pane_id = pane.pane_target().into_string();
                if seen_pane_ids.insert(pane_id.clone(), ()).is_some() {
                    return Err(SnapshotError::DuplicatePaneId { pane_id }.into());
                }

                let content_ref = pane_id.clone();
                if pane_table.contains_key(&content_ref) {
                    return Err(SnapshotError::DuplicateContentRef { content_ref }.into());
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
                    command_tree: pane.command_tree.as_ref().map(snapshot_from_process),
                });
            }

            windows.push(SnapshotWindow {
                id: window.window_id,
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
        backup_id: tmux.backup_id.clone(),
        created_at: tmux.create_time.clone(),
        sessions,
        pane_table,
    })
}

fn build_summary(
    tmux: &Tmux,
    manifest: &SnapshotManifestFile,
    manifest_bytes: &[u8],
) -> Result<SnapshotSummaryFile> {
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
) -> Result<()> {
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

fn write_json_file<T>(path: &Path, value: &T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| SnapshotError::SnapshotJson {
        path: path.to_path_buf(),
        source,
    })?;
    fs_ops::write_bytes(path, &bytes, io_error)?;
    sync_file(path)?;
    Ok(bytes)
}

fn read_summary_file(path: &Path) -> Result<SnapshotSummaryFile> {
    let bytes = read_file(path)?;
    let summary = read_json_slice::<SnapshotSummaryFile>(path, &bytes)?;
    ensure_supported_version(summary.schema_version, path)?;
    if summary.session_count as usize != summary.session_names.len() {
        return Err(SnapshotError::InvalidSummary {
            path: path.to_path_buf(),
            detail: "session_count does not match session_names length".to_string(),
        }
        .into());
    }
    Ok(summary)
}

fn parse_manifest_file(path: &Path, bytes: &[u8]) -> Result<SnapshotManifestFile> {
    let manifest = read_json_slice::<SnapshotManifestFile>(path, bytes)?;
    ensure_supported_version(manifest.schema_version, path)?;
    Ok(manifest)
}

fn validate_summary_matches_manifest(
    summary: &SnapshotSummaryFile,
    manifest: &SnapshotManifestFile,
    summary_path: &Path,
    manifest_path: &Path,
) -> Result<()> {
    if summary.backup_id != manifest.backup_id {
        return Err(summary_manifest_mismatch(
            summary_path.to_path_buf(),
            manifest_path.to_path_buf(),
            "backup_id differs between summary and manifest",
        ));
    }
    if summary.created_at != manifest.created_at {
        return Err(summary_manifest_mismatch(
            summary_path.to_path_buf(),
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
) -> Error {
    SnapshotError::SummaryManifestMismatch {
        summary_path,
        manifest_path,
        detail: detail.into(),
    }
    .into()
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs_ops::read_bytes(path, io_error)
}

fn create_dir_all(path: &Path) -> Result<()> {
    fs_ops::create_dir_all(path, io_error)
}

fn sync_file(path: &Path) -> Result<()> {
    fs_ops::sync_file(path, io_error)
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<()> {
    fs_ops::sync_dir(path, io_error)
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<()> {
    Ok(())
}

fn validate_relative_path(relative_path: &str) -> Result<()> {
    let path = Path::new(relative_path);
    if path.as_os_str().is_empty() {
        return Err(SnapshotError::InvalidRelativePath {
            relative_path: relative_path.to_string(),
        }
        .into());
    }

    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(SnapshotError::InvalidRelativePath {
                    relative_path: relative_path.to_string(),
                }
                .into());
            }
        }
    }

    Ok(())
}

pub(crate) fn validated_relative_path(relative_path: &str) -> Result<PathBuf> {
    validate_relative_path(relative_path)?;
    Ok(PathBuf::from(relative_path))
}

fn io_error(path: PathBuf, source: io::Error) -> Error {
    SnapshotError::SnapshotIo { path, source }.into()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hexadecimal = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hexadecimal, "{byte:02x}");
    }
    hexadecimal
}

#[cfg(test)]
mod publish_fault_tests {
    use super::*;
    use crate::model::{Pane, Session, Window};
    use crate::{Category, Code};
    use std::collections::BTreeMap;
    use std::fs;

    fn scratch(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "remux-snapshot-fault-{label}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("scratch");
        path
    }

    fn one_pane() -> (Tmux, BTreeMap<String, Vec<u8>>) {
        let mut tmux = Tmux::new("backup_20240101_120000");
        tmux.create_time = "2024-01-01 12:00:00".to_string();
        let mut session = Session::new("work");
        let mut window = Window::new("work", 1);
        let mut pane = Pane::new("work", 1, 0);
        pane.path = "/tmp/work".to_string();
        window.panes.push(pane);
        session.windows.push(window);
        tmux.sessions.push(session);
        let mut panes = BTreeMap::new();
        panes.insert("work:1.0".to_string(), b"pane\n".to_vec());
        (tmux, panes)
    }

    #[test]
    fn parent_fsync_failure_keeps_published_snapshot() {
        let parent = scratch("fsync");
        let dest = parent.join("backup_20240101_120000");
        let (tmux, panes) = one_pane();
        let _fail = fs_ops::inject::fail_sync_dir(parent.clone());

        let error = write_snapshot_dir(&dest, &tmux, &panes)
            .expect_err("parent fsync failure must surface");
        assert_eq!(error.category(), Category::Snapshot);
        assert!(matches!(
            error.code(),
            Code::Snapshot(SnapshotError::SnapshotIo { .. })
        ));
        let published = format!(
            "snapshot {} was published, but failed to sync catalog directory {}",
            dest.display(),
            parent.display()
        );
        assert_eq!(error.contexts().collect::<Vec<_>>(), [published.as_str()]);
        assert!(
            dest.join(SUMMARY_FILE_NAME).is_file(),
            "published snapshot must remain after parent fsync failure"
        );
        let leftover_staging = fs::read_dir(&parent)
            .expect("parent")
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".backup_20240101_120000.tmp-")
            });
        assert!(
            !leftover_staging,
            "must not try to delete staging after land"
        );
        let _ = fs::remove_dir_all(&parent);
    }

    #[test]
    fn unpublished_cleanup_failure_keeps_primary_code() {
        let parent = scratch("cleanup");
        let dest = parent.join("backup_20240101_120000");
        let mut tmux = Tmux::new("backup_20240101_120000");
        tmux.create_time = "2024-01-01 12:00:00".to_string();
        let mut session = Session::new("work");
        let mut window = Window::new("work", 1);
        window.panes.push(Pane::new("work", 1, 0));
        session.windows.push(window);
        tmux.sessions.push(session);
        let _fail = fs_ops::inject::fail_remove_dir_all();

        let error = write_snapshot_dir(&dest, &tmux, &BTreeMap::new())
            .expect_err("missing pane bytes fail before land");
        assert!(matches!(
            error.code(),
            Code::Snapshot(SnapshotError::MissingPaneBytes { .. })
        ));
        let contexts = error.contexts().collect::<Vec<_>>();
        assert_eq!(contexts.len(), 1);
        assert!(
            contexts[0].contains("also failed to remove temp snapshot"),
            "cleanup failure is one context entry, got {contexts:?}"
        );
        assert!(!dest.exists(), "must not publish on unpublished failure");
        let _ = fs::remove_dir_all(&parent);
    }
}
