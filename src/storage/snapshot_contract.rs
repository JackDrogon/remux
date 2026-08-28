use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::{Pane, Process, Session, Size, Tmux, Window};
use crate::{Result, Snapshot as SnapshotError};

use super::snapshot::{PaneAsset, SUMMARY_FILE_NAME};

const SNAPSHOT_SCHEMA_MAJOR: u16 = 1;
const SNAPSHOT_SCHEMA_MINOR: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SchemaVersion {
    pub(crate) major: u16,
    pub(crate) minor: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotSummaryFile {
    pub(crate) schema_version: SchemaVersion,
    pub(crate) backup_id: String,
    pub(crate) created_at: String,
    pub(crate) session_count: u32,
    pub(crate) window_count: u32,
    pub(crate) pane_count: u32,
    pub(crate) session_names: Vec<String>,
    pub(crate) active_session: Option<String>,
    pub(crate) manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotManifestFile {
    pub(crate) schema_version: SchemaVersion,
    pub(crate) backup_id: String,
    pub(crate) created_at: String,
    pub(crate) sessions: Vec<SnapshotSession>,
    pub(crate) pane_table: BTreeMap<String, PaneContentMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotSession {
    pub(crate) name: String,
    pub(crate) attached: bool,
    pub(crate) size: SnapshotSize,
    pub(crate) windows: Vec<SnapshotWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotWindow {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) active: bool,
    pub(crate) layout: String,
    pub(crate) panes: Vec<SnapshotPane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotPane {
    pub(crate) pane_id: u32,
    pub(crate) active: bool,
    pub(crate) path: String,
    pub(crate) size: SnapshotSize,
    pub(crate) content_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) command_tree: Option<SnapshotProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotProcess {
    pub(crate) name: String,
    pub(crate) argv: Vec<String>,
    pub(crate) pid: u32,
    #[serde(default)]
    pub(crate) foreground: bool,
    #[serde(default)]
    pub(crate) children: Vec<SnapshotProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaneContentMeta {
    pub(crate) relative_path: String,
    pub(crate) encoding: PaneEncoding,
    pub(crate) byte_len: u64,
    pub(crate) sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaneEncoding {
    #[serde(rename = "utf8")]
    Utf8,
    #[serde(rename = "binary")]
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SnapshotSize {
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
}

pub(crate) fn summary_to_tmux(summary: &SnapshotSummaryFile) -> Tmux {
    Tmux {
        backup_id: summary.backup_id.clone(),
        sessions: summary
            .session_names
            .iter()
            .map(|name| Session::new(name.clone()))
            .collect(),
        create_time: summary.created_at.clone(),
    }
}

pub(crate) fn current_version() -> SchemaVersion {
    SchemaVersion {
        major: SNAPSHOT_SCHEMA_MAJOR,
        minor: SNAPSHOT_SCHEMA_MINOR,
    }
}

pub(crate) fn summary_count(value: usize, field: &'static str) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        SnapshotError::InvalidSummary {
            path: PathBuf::from(SUMMARY_FILE_NAME),
            detail: format!("{field} exceeds u32 range"),
        }
        .into()
    })
}

pub(crate) fn read_json_slice<T>(path: &Path, bytes: &[u8]) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_slice(bytes).map_err(|source| {
        SnapshotError::SnapshotJson {
            path: path.to_path_buf(),
            source,
        }
        .into()
    })
}

pub(crate) fn ensure_supported_version(version: SchemaVersion, path: &Path) -> Result<()> {
    if version.major != SNAPSHOT_SCHEMA_MAJOR {
        return Err(SnapshotError::UnsupportedVersion {
            path: path.to_path_buf(),
            found_major: version.major,
        }
        .into());
    }
    Ok(())
}

pub(crate) fn build_loaded_snapshot(
    snapshot_dir: &Path,
    manifest: SnapshotManifestFile,
) -> Result<(Tmux, BTreeMap<String, PaneAsset>)> {
    let mut tmux = Tmux {
        backup_id: manifest.backup_id.clone(),
        sessions: Vec::new(),
        create_time: manifest.created_at.clone(),
    };
    let mut pane_assets = BTreeMap::new();

    for session in manifest.sessions {
        let mut model_windows = Vec::new();

        for window in session.windows {
            let mut model_panes = Vec::new();

            for pane in window.panes {
                let meta = manifest.pane_table.get(&pane.content_ref).ok_or_else(|| {
                    SnapshotError::InvalidManifest {
                        path: snapshot_dir.join(super::snapshot::MANIFEST_FILE_NAME),
                        detail: format!("missing pane_table entry for {}", pane.content_ref),
                    }
                })?;

                let pane_id =
                    crate::model::PaneTarget::from_parts(&session.name, window.id, pane.pane_id)
                        .into_string();
                if pane_assets.contains_key(&pane_id) {
                    return Err(SnapshotError::DuplicatePaneId { pane_id }.into());
                }

                let relative_path = super::snapshot::validated_relative_path(&meta.relative_path)?;
                pane_assets.insert(
                    pane_id.clone(),
                    PaneAsset {
                        relative_path,
                        byte_len: meta.byte_len,
                        sha256: meta.sha256.clone(),
                        encoding: meta.encoding,
                    },
                );

                model_panes.push(Pane {
                    pane_id: pane.pane_id,
                    size: pane.size.into_size(),
                    path: pane.path,
                    active: pane.active,
                    session_name: session.name.clone(),
                    window_id: window.id,
                    command_tree: pane.command_tree.map(process_from_snapshot),
                });
            }

            model_windows.push(Window {
                window_id: window.id,
                name: window.name,
                panes: model_panes,
                active: window.active,
                session_name: session.name.clone(),
                layout: window.layout,
            });
        }

        tmux.sessions.push(Session {
            name: session.name,
            attached: session.attached,
            size: session.size.into_size(),
            windows: model_windows,
        });
    }

    Ok((tmux, pane_assets))
}

fn process_from_snapshot(process: SnapshotProcess) -> Process {
    Process {
        name: process.name,
        argv: process.argv,
        pid: process.pid,
        foreground: process.foreground,
        children: process
            .children
            .into_iter()
            .map(process_from_snapshot)
            .collect(),
    }
}

pub(crate) fn snapshot_from_process(process: &Process) -> SnapshotProcess {
    SnapshotProcess {
        name: process.name.clone(),
        argv: process.argv.clone(),
        pid: process.pid,
        foreground: process.foreground,
        children: process.children.iter().map(snapshot_from_process).collect(),
    }
}

impl SnapshotSize {
    pub(crate) fn from_size(size: Size) -> Self {
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

    pub(crate) fn into_size(self) -> Size {
        match (self.width, self.height) {
            (Some(width), Some(height)) => Size::new(width, height),
            _ => Size::empty(),
        }
    }
}
