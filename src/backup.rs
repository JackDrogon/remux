use std::ffi::{CStr, c_char, c_int, c_long};
use std::fmt;
use std::fs;
use std::io;
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::backup_name::BackupNameError;
use crate::config::AppState;
use crate::error::SubprocessError;
use crate::model::{Pane, Session, Size, Tmux, Window};
use crate::serde_legacy::{self, LegacySnapshotError};
use crate::tmux::{OUTPUT_SEPARATOR, TmuxAdapter, TmuxCommand};

const BACKUP_ID_TIME_FORMAT: &[u8] = b"%Y%m%d_%H%M%S\0";
const CREATE_TIME_FORMAT: &[u8] = b"%Y-%m-%d %H:%M:%S\0";
const TIME_BUFFER_SIZE: usize = 32;

type TimeT = c_long;

#[repr(C)]
#[derive(Clone, Copy)]
struct LocalTime {
    tm_sec: c_int,
    tm_min: c_int,
    tm_hour: c_int,
    tm_mday: c_int,
    tm_mon: c_int,
    tm_year: c_int,
    tm_wday: c_int,
    tm_yday: c_int,
    tm_isdst: c_int,
    tm_gmtoff: c_long,
    tm_zone: *const c_char,
}

unsafe extern "C" {
    fn localtime_r(timep: *const TimeT, result: *mut LocalTime) -> *mut LocalTime;
    fn strftime(
        buffer: *mut c_char,
        max_size: usize,
        format: *const c_char,
        time: *const LocalTime,
    ) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupOutcome {
    Created { backup_id: String, path: PathBuf },
    NoServer,
}

#[derive(Debug)]
pub enum BackupError {
    DuplicateBackupId {
        backup_id: String,
    },
    InvalidBackupName(BackupNameError),
    Tmux(SubprocessError),
    Snapshot(LegacySnapshotError),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    InvalidTmuxOutput {
        command: &'static str,
        line: String,
        detail: String,
    },
    TimestampBeforeEpoch,
    TimestampOutOfRange,
    TimestampFormattingFailed,
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateBackupId { .. } => {
                write!(
                    f,
                    "(backup -b):the given backup name exists already, aborted."
                )
            }
            Self::InvalidBackupName(error) => write!(f, "{error}"),
            Self::Tmux(error) => write!(f, "{error}"),
            Self::Snapshot(error) => write!(f, "{error}"),
            Self::Io { path, source } => {
                write!(f, "failed to write {}: {source}", path.display())
            }
            Self::InvalidTmuxOutput {
                command,
                line,
                detail,
            } => write!(f, "invalid tmux {command} output: {detail} (line: {line})"),
            Self::TimestampBeforeEpoch => {
                write!(f, "current system time is before the UNIX epoch")
            }
            Self::TimestampOutOfRange => write!(f, "current system time is out of range"),
            Self::TimestampFormattingFailed => {
                write!(f, "failed to format the current local timestamp")
            }
        }
    }
}

impl std::error::Error for BackupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidBackupName(error) => Some(error),
            Self::Tmux(error) => Some(error),
            Self::Snapshot(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::DuplicateBackupId { .. }
            | Self::InvalidTmuxOutput { .. }
            | Self::TimestampBeforeEpoch
            | Self::TimestampOutOfRange
            | Self::TimestampFormattingFailed => None,
        }
    }
}

impl From<SubprocessError> for BackupError {
    fn from(value: SubprocessError) -> Self {
        Self::Tmux(value)
    }
}

impl From<LegacySnapshotError> for BackupError {
    fn from(value: LegacySnapshotError) -> Self {
        Self::Snapshot(value)
    }
}

pub fn capture_backup(
    config: &AppState,
    requested_backup_id: Option<&str>,
) -> Result<BackupOutcome, BackupError> {
    let backup_id = resolve_backup_id(requested_backup_id)?;
    let backup_path = config.active_backup_path().join(&backup_id);

    if backup_path.exists() {
        return Err(BackupError::DuplicateBackupId { backup_id });
    }

    let adapter = TmuxAdapter::new(config);
    if !adapter.has_server()? {
        return Ok(BackupOutcome::NoServer);
    }

    let snapshot = load_snapshot(&adapter, &backup_id)?;
    let snapshot_file = backup_path.join(format!("{backup_id}.json"));
    serde_legacy::write_snapshot_file(&snapshot_file, &snapshot)?;

    for session in &snapshot.sessions {
        for window in &session.windows {
            for pane in &window.panes {
                let pane_file = backup_path.join(pane.idstr());
                write_pane_capture(
                    &adapter,
                    pane,
                    config.config().capture.with_escape,
                    &pane_file,
                )?;
            }
        }
    }

    Ok(BackupOutcome::Created {
        backup_id,
        path: backup_path,
    })
}

fn resolve_backup_id(requested_backup_id: Option<&str>) -> Result<String, BackupError> {
    match requested_backup_id {
        Some(backup_id) => crate::backup_name::normalize_backup_name(backup_id)
            .map_err(BackupError::InvalidBackupName),
        None => format_local_time(BACKUP_ID_TIME_FORMAT),
    }
}

fn load_snapshot(adapter: &TmuxAdapter, backup_id: &str) -> Result<Tmux, BackupError> {
    let mut tmux = Tmux::new(backup_id);
    tmux.create_time = format_local_time(CREATE_TIME_FORMAT)?;
    tmux.sessions = load_sessions(adapter)?;
    Ok(tmux)
}

fn load_sessions(adapter: &TmuxAdapter) -> Result<Vec<Session>, BackupError> {
    adapter
        .list_sessions()?
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| parse_session(adapter, &line))
        .collect()
}

fn parse_session(adapter: &TmuxAdapter, line: &str) -> Result<Session, BackupError> {
    let fields = split_fields("list-sessions", line, 3)?;
    let mut session = Session::new(fields[0]);
    session.size = parse_size(fields[1], "list-sessions", line)?;
    session.attached = parse_active(fields[2], "list-sessions", line)?;
    session.windows = load_windows(adapter, &session.name)?;
    Ok(session)
}

fn load_windows(adapter: &TmuxAdapter, session_name: &str) -> Result<Vec<Window>, BackupError> {
    adapter
        .list_windows(session_name)?
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| parse_window(adapter, session_name, &line))
        .collect()
}

fn parse_window(
    adapter: &TmuxAdapter,
    session_name: &str,
    line: &str,
) -> Result<Window, BackupError> {
    let fields = split_fields("list-windows", line, 4)?;
    let window_id = parse_u32(fields[0], "list-windows", line)?;
    let mut window = Window::new(session_name, window_id);
    window.name = fields[1].to_string();
    window.active = parse_active(fields[2], "list-windows", line)?;
    window.layout = fields[3].to_string();
    window.panes = load_panes(adapter, session_name, window_id)?;
    Ok(window)
}

fn load_panes(
    adapter: &TmuxAdapter,
    session_name: &str,
    window_id: u32,
) -> Result<Vec<Pane>, BackupError> {
    adapter
        .list_panes(
            session_name,
            usize::try_from(window_id).map_err(|_| BackupError::InvalidTmuxOutput {
                command: "list-panes",
                line: format!("{session_name}:{window_id}"),
                detail: "window id exceeds usize range".to_string(),
            })?,
        )?
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| parse_pane(session_name, window_id, &line))
        .collect()
}

fn parse_pane(session_name: &str, window_id: u32, line: &str) -> Result<Pane, BackupError> {
    let fields = split_fields("list-panes", line, 4)?;
    let pane_id = parse_u32(fields[0], "list-panes", line)?;
    let mut pane = Pane::new(session_name, window_id, pane_id);
    pane.size = parse_size(fields[1], "list-panes", line)?;
    pane.path = fields[2].to_string();
    pane.active = parse_active(fields[3], "list-panes", line)?;
    Ok(pane)
}

fn split_fields<'a>(
    command: &'static str,
    line: &'a str,
    expected_len: usize,
) -> Result<Vec<&'a str>, BackupError> {
    let fields = line.split(OUTPUT_SEPARATOR).collect::<Vec<_>>();
    if fields.len() == expected_len {
        Ok(fields)
    } else {
        Err(BackupError::InvalidTmuxOutput {
            command,
            line: line.to_string(),
            detail: format!("expected {expected_len} fields, found {}", fields.len()),
        })
    }
}

fn parse_size(command_value: &str, command: &'static str, line: &str) -> Result<Size, BackupError> {
    let Some(inner) = command_value
        .trim()
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
    else {
        return Err(BackupError::InvalidTmuxOutput {
            command,
            line: line.to_string(),
            detail: format!("invalid size tuple: {command_value}"),
        });
    };

    if inner.trim().is_empty() {
        return Ok(Size::empty());
    }

    let Some((width, height)) = inner.split_once(',') else {
        return Err(BackupError::InvalidTmuxOutput {
            command,
            line: line.to_string(),
            detail: format!("invalid size tuple: {command_value}"),
        });
    };

    Ok(Size::new(
        parse_u32(width.trim(), command, line)?,
        parse_u32(height.trim(), command, line)?,
    ))
}

fn parse_u32(value: &str, command: &'static str, line: &str) -> Result<u32, BackupError> {
    value
        .parse::<u32>()
        .map_err(|error| BackupError::InvalidTmuxOutput {
            command,
            line: line.to_string(),
            detail: format!("failed to parse integer {value:?}: {error}"),
        })
}

fn parse_active(value: &str, command: &'static str, line: &str) -> Result<bool, BackupError> {
    value
        .parse::<i64>()
        .map(|value| value > 0)
        .map_err(|error| BackupError::InvalidTmuxOutput {
            command,
            line: line.to_string(),
            detail: format!("failed to parse active flag {value:?}: {error}"),
        })
}

fn write_pane_capture(
    adapter: &TmuxAdapter,
    pane: &Pane,
    include_escape: bool,
    path: &Path,
) -> Result<(), BackupError> {
    let command = adapter.render_command(TmuxCommand::CapturePane {
        pane_id: pane.idstr(),
        include_escape,
    });
    let output = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .output()
        .map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => BackupError::Tmux(SubprocessError::BinaryNotFound {
                command: command.clone(),
                source,
            }),
            _ => BackupError::Tmux(SubprocessError::SpawnFailed {
                command: command.clone(),
                source,
            }),
        })?;

    if !output.status.success() {
        return Err(BackupError::Tmux(SubprocessError::Failed {
            command,
            status: output.status.code(),
            stdout: normalize_stream(output.stdout),
            stderr: normalize_stream(output.stderr),
        }));
    }

    fs::write(path, output.stdout).map_err(|source| BackupError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn normalize_stream(bytes: Vec<u8>) -> String {
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.ends_with('\n') {
        text.pop();
    }
    text
}

fn format_local_time(format: &'static [u8]) -> Result<String, BackupError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BackupError::TimestampBeforeEpoch)?
        .as_secs();
    let seconds = TimeT::try_from(seconds).map_err(|_| BackupError::TimestampOutOfRange)?;

    let mut local_time = MaybeUninit::<LocalTime>::uninit();
    let result = unsafe { localtime_r(&seconds, local_time.as_mut_ptr()) };
    if result.is_null() {
        return Err(BackupError::TimestampFormattingFailed);
    }

    let local_time = unsafe { local_time.assume_init() };
    let mut buffer = [0u8; TIME_BUFFER_SIZE];
    let written = unsafe {
        strftime(
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            format.as_ptr().cast(),
            &local_time,
        )
    };
    if written == 0 {
        return Err(BackupError::TimestampFormattingFailed);
    }

    let formatted = unsafe { CStr::from_ptr(buffer.as_ptr().cast()) };
    Ok(formatted.to_string_lossy().into_owned())
}
