use std::io;
use std::path::PathBuf;

use thiserror::Error;
use xerror::Context;

#[derive(Debug, Error)]
pub enum Backup {
    #[error(
        "backup aborted: the given backup name already exists. name:{backup_id} path:{}",
        path.display()
    )]
    DuplicateBackupId { backup_id: String, path: PathBuf },
    #[error("failed to discover tmux sockets")]
    SocketDiscovery {
        #[source]
        source: io::Error,
    },
    #[error("invalid tmux {command} output: {detail} (line: {line})")]
    InvalidTmuxOutput {
        command: &'static str,
        line: String,
        detail: String,
    },
}

#[derive(Debug, Error)]
pub enum Catalog {
    #[error("invalid backup name: backup name cannot be empty or whitespace-only")]
    BackupNameEmpty,
    #[error("invalid backup name {raw:?}: backup name cannot be '..'")]
    BackupNameParentTraversal { raw: String },
    #[error("invalid backup name {raw:?}: backup name cannot contain path separators")]
    BackupNamePathSeparator { raw: String },
    #[error("invalid backup name {raw:?}: backup name cannot be absolute-like")]
    BackupNameAbsoluteLike { raw: String },
    #[error("failed to read backup catalog {}", path.display())]
    ReadCatalog {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// Filesystem metadata of a backup path (catalog mtime and backup-id slot occupancy).
    #[error("failed to read backup metadata {}", path.display())]
    ReadMetadata {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("backup catalog root is not a directory: {}", path.display())]
    RootNotDirectory { path: PathBuf },
    #[error("cannot find given backup name:{name} under {}", root.display())]
    MissingBackupName { name: String, root: PathBuf },
    #[error("backup dir is empty under {}, nothing to resolve", root.display())]
    NoBackups { root: PathBuf },
    #[error("failed to delete backup {}", path.display())]
    DeleteBackup {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Error)]
pub enum Cli {
    #[error("failed to write stdout")]
    Stdout(#[source] io::Error),
}

#[derive(Debug, Error)]
pub enum Config {
    #[error("HOME is not set; cannot resolve ~/.remux")]
    HomeDirNotFound,
    #[error("failed to create {}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to read {}", path.display())]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse config {}", path.display())]
    ParseToml {
        path: PathBuf,
        #[source]
        source: Box<toml::de::Error>,
    },
    #[error("tmux.binary must not be empty")]
    InvalidTmuxBinary,
    #[error("backup.{field} must be a single path component: {value:?}")]
    InvalidBackupDirName { field: &'static str, value: String },
    #[error("failed to write {}", path.display())]
    WriteFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Error)]
pub enum Interactive {
    #[error("interactive I/O failed")]
    InteractiveIo(#[source] io::Error),
    #[error("end of input while reading {context}")]
    EndOfInput { context: &'static str },
}

#[derive(Debug, Error)]
pub enum Restore {
    #[error("invalid tmux base-index value {raw:?}")]
    InvalidBaseIndex {
        raw: String,
        #[source]
        source: std::num::ParseIntError,
    },
    #[error("missing pane metadata for {pane_id}")]
    MissingPaneAsset { pane_id: String },
}

#[derive(Debug, Error)]
pub enum Snapshot {
    #[error("I/O error at {}", path.display())]
    SnapshotIo {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("JSON error at {}", path.display())]
    SnapshotJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("unsupported snapshot schema major version {found_major} at {}", path.display())]
    UnsupportedVersion { path: PathBuf, found_major: u16 },
    #[error("invalid snapshot summary {}: {detail}", path.display())]
    InvalidSummary { path: PathBuf, detail: String },
    #[error("invalid snapshot manifest {}: {detail}", path.display())]
    InvalidManifest { path: PathBuf, detail: String },
    #[error(
        "summary/manifest mismatch between {} and {}: {detail}",
        summary_path.display(),
        manifest_path.display()
    )]
    SummaryManifestMismatch {
        summary_path: PathBuf,
        manifest_path: PathBuf,
        detail: String,
    },
    #[error("missing captured pane bytes for {pane_id}")]
    MissingPaneBytes { pane_id: String },
    #[error("duplicate pane id in snapshot model: {pane_id}")]
    DuplicatePaneId { pane_id: String },
    #[error("duplicate content_ref in snapshot manifest: {content_ref}")]
    DuplicateContentRef { content_ref: String },
    #[error("invalid relative snapshot path: {relative_path}")]
    InvalidRelativePath { relative_path: String },
    #[error("missing pane content for {pane_id} at {}", path.display())]
    MissingPaneContent { pane_id: String, path: PathBuf },
    #[error("invalid pane content for {pane_id} at {}: {detail}", path.display())]
    InvalidPaneContent {
        pane_id: String,
        path: PathBuf,
        detail: String,
    },
}

#[derive(Debug, Error)]
pub enum Tmux {
    #[error("subprocess binary not found for {}", format_command(command))]
    BinaryNotFound {
        command: Vec<String>,
        #[source]
        source: io::Error,
    },
    #[error("failed to spawn subprocess {}", format_command(command))]
    SpawnFailed {
        command: Vec<String>,
        #[source]
        source: io::Error,
    },
    #[error("failed while waiting for subprocess {}", format_command(command))]
    WaitFailed {
        command: Vec<String>,
        #[source]
        source: io::Error,
    },
    // No timed-out variant: the adapter waits for tmux to exit. See
    // `SubprocessRunner`.
    #[error("{}", failed_message(.command, *status, stderr))]
    TmuxFailed {
        command: Vec<String>,
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

fn format_command(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part.is_empty() || part.chars().any(char::is_whitespace) {
                format!("{part:?}")
            } else {
                part.clone()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn format_status(status: Option<i32>) -> String {
    status
        .map(|value| value.to_string())
        .unwrap_or_else(|| "signal".to_string())
}

fn failed_message(command: &[String], status: Option<i32>, stderr: &str) -> String {
    let status = format_status(status);
    if stderr.is_empty() {
        format!(
            "subprocess exited with status {status}: {}",
            format_command(command)
        )
    } else {
        format!(
            "subprocess exited with status {status}: {} (stderr: {})",
            format_command(command),
            stderr
        )
    }
}

#[xerror::code]
#[derive(Debug)]
pub enum Code {
    Cli,
    Config,
    Catalog,
    Snapshot,
    Backup,
    Restore,
    Tmux,
    Interactive,
}

pub type Error = xerror::Error<Code>;
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn attach_context(error: Error, msg: impl std::fmt::Display) -> Error {
    // xerror Context is Result-only. Only for a primary failure plus cleanup.
    Err::<(), Error>(error).context(msg).unwrap_err()
}
