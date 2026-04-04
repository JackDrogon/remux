use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::model::{Pane, Session, Size, Tmux, Window};

pub const SUMMARY_FILE_NAME: &str = "summary.json";
pub const MANIFEST_FILE_NAME: &str = "manifest.json";
const PANES_DIR_NAME: &str = "panes";
const SNAPSHOT_SCHEMA_MAJOR: u16 = 1;
const SNAPSHOT_SCHEMA_MINOR: u16 = 0;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct SchemaVersion {
    major: u16,
    minor: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotSummaryFile {
    schema_version: SchemaVersion,
    backup_id: String,
    created_at: String,
    session_count: u32,
    window_count: u32,
    pane_count: u32,
    session_names: Vec<String>,
    active_session: Option<String>,
    manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotManifestFile {
    schema_version: SchemaVersion,
    backup_id: String,
    created_at: String,
    sessions: Vec<SnapshotSession>,
    pane_table: BTreeMap<String, PaneContentMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotSession {
    name: String,
    attached: bool,
    size: SnapshotSize,
    windows: Vec<SnapshotWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotWindow {
    id: u32,
    name: String,
    active: bool,
    layout: String,
    panes: Vec<SnapshotPane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotPane {
    pane_id: u32,
    active: bool,
    path: String,
    size: SnapshotSize,
    content_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct PaneContentMeta {
    relative_path: String,
    encoding: PaneEncoding,
    byte_len: u64,
    sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneEncoding {
    #[serde(rename = "utf8")]
    Utf8,
    #[serde(rename = "binary")]
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotSize {
    width: Option<u32>,
    height: Option<u32>,
}

pub fn write_snapshot_dir(
    snapshot_dir: &Path,
    tmux: &Tmux,
    pane_contents: &BTreeMap<String, Vec<u8>>,
) -> Result<(), SnapshotError> {
    let parent = snapshot_dir
        .parent()
        .ok_or_else(|| SnapshotError::InvalidManifest {
            path: snapshot_dir.to_path_buf(),
            detail: "snapshot directory must have a parent".to_string(),
        })?;
    create_dir_all(parent)?;

    let temp_dir = prepare_temp_dir(snapshot_dir)?;
    let panes_dir = temp_dir.join(PANES_DIR_NAME);
    create_dir_all(&panes_dir)?;

    let manifest = build_manifest(tmux, pane_contents)?;
    write_pane_files(&temp_dir, pane_contents, &manifest.pane_table)?;

    let manifest_path = temp_dir.join(MANIFEST_FILE_NAME);
    let manifest_bytes = write_json_file(&manifest_path, &manifest)?;
    let summary = build_summary(tmux, &manifest, &manifest_bytes)?;
    let summary_path = temp_dir.join(SUMMARY_FILE_NAME);
    write_json_file(&summary_path, &summary)?;

    sync_dir(&panes_dir)?;
    sync_dir(&temp_dir)?;
    fs::rename(&temp_dir, snapshot_dir).map_err(|source| SnapshotError::Io {
        path: snapshot_dir.to_path_buf(),
        source,
    })?;
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
        return Err(SnapshotError::SummaryManifestMismatch {
            summary_path,
            manifest_path: manifest_path.clone(),
            detail: format!(
                "expected manifest sha {}, found {}",
                summary.manifest_sha256, actual_hash
            ),
        });
    }

    let manifest = parse_manifest_file(&manifest_path, &manifest_bytes)?;
    validate_summary_matches_manifest(&summary, &manifest, &manifest_path)?;
    build_loaded_snapshot(snapshot_dir, manifest)
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
        fs::remove_dir_all(&temp_dir).map_err(|source| SnapshotError::Io {
            path: temp_dir.clone(),
            source,
        })?;
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

                let content_ref = pane.idstr();
                if pane_table.contains_key(&content_ref) {
                    return Err(SnapshotError::DuplicateContentRef { content_ref });
                }

                let content = pane_contents.get(&pane.idstr()).ok_or_else(|| {
                    SnapshotError::MissingPaneBytes {
                        pane_id: pane.idstr(),
                    }
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
        session_count: u32::try_from(session_count).map_err(|_| SnapshotError::InvalidSummary {
            path: PathBuf::from(SUMMARY_FILE_NAME),
            detail: "session_count exceeds u32 range".to_string(),
        })?,
        window_count: u32::try_from(window_count).map_err(|_| SnapshotError::InvalidSummary {
            path: PathBuf::from(SUMMARY_FILE_NAME),
            detail: "window_count exceeds u32 range".to_string(),
        })?,
        pane_count: u32::try_from(pane_count).map_err(|_| SnapshotError::InvalidSummary {
            path: PathBuf::from(SUMMARY_FILE_NAME),
            detail: "pane_count exceeds u32 range".to_string(),
        })?,
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
        fs::write(&pane_path, pane_bytes).map_err(|source| SnapshotError::Io {
            path: pane_path.clone(),
            source,
        })?;
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
    fs::write(path, &bytes).map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    sync_file(path)?;
    Ok(bytes)
}

fn read_summary_file(path: &Path) -> Result<SnapshotSummaryFile, SnapshotError> {
    let bytes = read_file(path)?;
    let summary = serde_json::from_slice::<SnapshotSummaryFile>(&bytes).map_err(|source| {
        SnapshotError::Json {
            path: path.to_path_buf(),
            source,
        }
    })?;
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
    let manifest = serde_json::from_slice::<SnapshotManifestFile>(bytes).map_err(|source| {
        SnapshotError::Json {
            path: path.to_path_buf(),
            source,
        }
    })?;
    ensure_supported_version(manifest.schema_version, path)?;
    Ok(manifest)
}

fn validate_summary_matches_manifest(
    summary: &SnapshotSummaryFile,
    manifest: &SnapshotManifestFile,
    manifest_path: &Path,
) -> Result<(), SnapshotError> {
    if summary.backup_id != manifest.backup_id {
        return Err(SnapshotError::SummaryManifestMismatch {
            summary_path: PathBuf::from(SUMMARY_FILE_NAME),
            manifest_path: manifest_path.to_path_buf(),
            detail: "backup_id differs between summary and manifest".to_string(),
        });
    }
    if summary.created_at != manifest.created_at {
        return Err(SnapshotError::SummaryManifestMismatch {
            summary_path: PathBuf::from(SUMMARY_FILE_NAME),
            manifest_path: manifest_path.to_path_buf(),
            detail: "created_at differs between summary and manifest".to_string(),
        });
    }
    Ok(())
}

fn build_loaded_snapshot(
    snapshot_dir: &Path,
    manifest: SnapshotManifestFile,
) -> Result<LoadedSnapshot, SnapshotError> {
    let mut tmux = Tmux::new(manifest.backup_id.clone());
    tmux.create_time = manifest.created_at.clone();
    let mut pane_assets = BTreeMap::new();

    for session in manifest.sessions {
        let mut model_session = Session::new(session.name.clone());
        model_session.attached = session.attached;
        model_session.size = session.size.into_size();

        for window in session.windows {
            let mut model_window = Window::new(&session.name, window.id);
            model_window.name = window.name;
            model_window.active = window.active;
            model_window.layout = window.layout;

            for pane in window.panes {
                let meta = manifest.pane_table.get(&pane.content_ref).ok_or_else(|| {
                    SnapshotError::InvalidManifest {
                        path: snapshot_dir.join(MANIFEST_FILE_NAME),
                        detail: format!("missing pane_table entry for {}", pane.content_ref),
                    }
                })?;

                let pane_id = format!("{}:{}.{}", session.name, window.id, pane.pane_id);
                if pane_assets.contains_key(&pane_id) {
                    return Err(SnapshotError::DuplicatePaneId { pane_id });
                }

                let relative_path = validated_relative_path(&meta.relative_path)?;
                pane_assets.insert(
                    pane_id.clone(),
                    PaneAsset {
                        relative_path,
                        byte_len: meta.byte_len,
                        sha256: meta.sha256.clone(),
                        encoding: meta.encoding,
                    },
                );

                let mut model_pane = Pane::new(&session.name, window.id, pane.pane_id);
                model_pane.active = pane.active;
                model_pane.path = pane.path;
                model_pane.size = pane.size.into_size();
                model_window.panes.push(model_pane);
            }

            model_session.windows.push(model_window);
        }

        tmux.sessions.push(model_session);
    }

    Ok(LoadedSnapshot { tmux, pane_assets })
}

fn summary_to_tmux(summary: &SnapshotSummaryFile) -> Tmux {
    let mut tmux = Tmux::new(summary.backup_id.clone());
    tmux.create_time = summary.created_at.clone();
    tmux.sessions = summary
        .session_names
        .iter()
        .map(|name| Session::new(name.clone()))
        .collect();
    tmux
}

fn current_version() -> SchemaVersion {
    SchemaVersion {
        major: SNAPSHOT_SCHEMA_MAJOR,
        minor: SNAPSHOT_SCHEMA_MINOR,
    }
}

fn ensure_supported_version(version: SchemaVersion, path: &Path) -> Result<(), SnapshotError> {
    if version.major != SNAPSHOT_SCHEMA_MAJOR {
        return Err(SnapshotError::UnsupportedVersion {
            path: path.to_path_buf(),
            found_major: version.major,
        });
    }
    Ok(())
}

fn read_file(path: &Path) -> Result<Vec<u8>, SnapshotError> {
    fs::read(path).map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir_all(path: &Path) -> Result<(), SnapshotError> {
    fs::create_dir_all(path).map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn sync_file(path: &Path) -> Result<(), SnapshotError> {
    let file = File::open(path).map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> Result<(), SnapshotError> {
    let file = File::open(path).map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.sync_all().map_err(|source| SnapshotError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> Result<(), SnapshotError> {
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
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
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir | Component::ParentDir => {
                return Err(SnapshotError::InvalidRelativePath {
                    relative_path: relative_path.to_string(),
                });
            }
        }
    }

    Ok(())
}

fn validated_relative_path(relative_path: &str) -> Result<PathBuf, SnapshotError> {
    validate_relative_path(relative_path)?;
    Ok(PathBuf::from(relative_path))
}

impl SnapshotSize {
    fn from_size(size: Size) -> Self {
        match size.as_tuple() {
            Some((width, height)) => Self {
                width: Some(width),
                height: Some(height),
            },
            None => Self {
                width: None,
                height: None,
            },
        }
    }

    fn into_size(self) -> Size {
        match (self.width, self.height) {
            (Some(width), Some(height)) => Size::new(width, height),
            _ => Size::empty(),
        }
    }
}
