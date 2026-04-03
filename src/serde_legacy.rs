use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::model::{Pane, Session, Size, Tmux, Window};

pub const LEGACY_MODULE_NAME: &str = "tmuxbk.tmux_obj";

#[derive(Debug)]
pub enum LegacySnapshotError {
    Io(std::io::Error),
    Json(serde_json::Error),
    ExpectedObject {
        path: String,
    },
    MissingMarker {
        path: String,
        marker: &'static str,
    },
    UnexpectedClass {
        path: String,
        expected: &'static str,
        found: String,
    },
    UnexpectedModule {
        path: String,
        expected: &'static str,
        found: String,
    },
    MissingField {
        path: String,
        field: &'static str,
    },
    InvalidFieldType {
        path: String,
        field: &'static str,
        detail: String,
    },
}

impl fmt::Display for LegacySnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Json(error) => write!(f, "JSON error: {error}"),
            Self::ExpectedObject { path } => {
                write!(f, "expected JSON object at {path}")
            }
            Self::MissingMarker { path, marker } => {
                write!(f, "missing required legacy marker {marker} at {path}")
            }
            Self::UnexpectedClass {
                path,
                expected,
                found,
            } => write!(
                f,
                "unexpected legacy class at {path}: expected {expected}, found {found}"
            ),
            Self::UnexpectedModule {
                path,
                expected,
                found,
            } => write!(
                f,
                "unexpected legacy module at {path}: expected {expected}, found {found}"
            ),
            Self::MissingField { path, field } => {
                write!(f, "missing required field {field} at {path}")
            }
            Self::InvalidFieldType {
                path,
                field,
                detail,
            } => write!(f, "invalid field {field} at {path}: {detail}"),
        }
    }
}

impl std::error::Error for LegacySnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::ExpectedObject { .. }
            | Self::MissingMarker { .. }
            | Self::UnexpectedClass { .. }
            | Self::UnexpectedModule { .. }
            | Self::MissingField { .. }
            | Self::InvalidFieldType { .. } => None,
        }
    }
}

impl From<std::io::Error> for LegacySnapshotError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for LegacySnapshotError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

pub fn from_reader<R>(reader: R) -> Result<Tmux, LegacySnapshotError>
where
    R: Read,
{
    let value: Value = serde_json::from_reader(reader)?;
    parse_tmux(&value, "$")
}

pub fn from_str(input: &str) -> Result<Tmux, LegacySnapshotError> {
    let value: Value = serde_json::from_str(input)?;
    parse_tmux(&value, "$")
}

pub fn read_snapshot_file<P>(path: P) -> Result<Tmux, LegacySnapshotError>
where
    P: AsRef<Path>,
{
    let file = File::open(path)?;
    from_reader(file)
}

pub fn to_string_pretty(tmux: &Tmux) -> Result<String, LegacySnapshotError> {
    Ok(serde_json::to_string_pretty(&tmux_to_value(tmux))?)
}

pub fn to_writer_pretty<W>(mut writer: W, tmux: &Tmux) -> Result<(), LegacySnapshotError>
where
    W: Write,
{
    serde_json::to_writer_pretty(&mut writer, &tmux_to_value(tmux))?;
    Ok(())
}

pub fn write_snapshot_file<P>(path: P, tmux: &Tmux) -> Result<(), LegacySnapshotError>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(path)?;
    to_writer_pretty(file, tmux)
}

fn parse_tmux(value: &Value, path: &str) -> Result<Tmux, LegacySnapshotError> {
    let object = expect_object(value, path)?;
    expect_markers(object, path, "Tmux")?;

    let mut tmux = Tmux::new(get_required_string(object, path, "tid")?);
    tmux.create_time = get_optional_string(object, path, "create_time")?.unwrap_or_default();
    tmux.sessions = parse_sessions(object.get("sessions"), path, "sessions")?;
    Ok(tmux)
}

fn parse_session(value: &Value, path: &str) -> Result<Session, LegacySnapshotError> {
    let object = expect_object(value, path)?;
    expect_markers(object, path, "Session")?;

    let mut session = Session::new(get_required_string(object, path, "name")?);
    session.attached = get_optional_bool(object, path, "attached")?.unwrap_or(false);
    session.size = get_optional_size(object, path, "size")?;
    session.windows = parse_windows(object.get("windows"), path, "windows")?;
    Ok(session)
}

fn parse_window(value: &Value, path: &str) -> Result<Window, LegacySnapshotError> {
    let object = expect_object(value, path)?;
    expect_markers(object, path, "Window")?;

    let sess_name = get_required_string(object, path, "sess_name")?;
    let win_id = get_required_u32(object, path, "win_id")?;
    let mut window = Window::new(sess_name, win_id);
    if let Some(name) = get_optional_string(object, path, "name")? {
        window.name = name;
    }
    window.active = get_optional_bool(object, path, "active")?.unwrap_or(false);
    window.layout = get_optional_string(object, path, "layout")?.unwrap_or_default();
    window.panes = parse_panes(object.get("panes"), path, "panes")?;
    Ok(window)
}

fn parse_pane(value: &Value, path: &str) -> Result<Pane, LegacySnapshotError> {
    let object = expect_object(value, path)?;
    expect_markers(object, path, "Pane")?;

    let sess_name = get_required_string(object, path, "sess_name")?;
    let win_id = get_required_u32(object, path, "win_id")?;
    let pane_id = get_required_u32(object, path, "pane_id")?;
    let mut pane = Pane::new(sess_name, win_id, pane_id);
    pane.size = get_optional_size(object, path, "size")?;
    pane.path = get_optional_string(object, path, "path")?.unwrap_or_else(|| "~".to_string());
    pane.active = get_optional_bool(object, path, "active")?.unwrap_or(false);
    pane.cont_file = get_optional_string(object, path, "cont_file")?.unwrap_or_default();
    Ok(pane)
}

fn expect_object<'a>(
    value: &'a Value,
    path: &str,
) -> Result<&'a Map<String, Value>, LegacySnapshotError> {
    value
        .as_object()
        .ok_or_else(|| LegacySnapshotError::ExpectedObject {
            path: path.to_string(),
        })
}

fn expect_markers(
    object: &Map<String, Value>,
    path: &str,
    expected_class: &'static str,
) -> Result<(), LegacySnapshotError> {
    let class_name = object
        .get("__class__")
        .ok_or_else(|| LegacySnapshotError::MissingMarker {
            path: path.to_string(),
            marker: "__class__",
        })?
        .as_str()
        .ok_or_else(|| LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field: "__class__",
            detail: "expected a string".to_string(),
        })?;

    if class_name != expected_class {
        return Err(LegacySnapshotError::UnexpectedClass {
            path: path.to_string(),
            expected: expected_class,
            found: class_name.to_string(),
        });
    }

    let module_name = object
        .get("__module__")
        .ok_or_else(|| LegacySnapshotError::MissingMarker {
            path: path.to_string(),
            marker: "__module__",
        })?
        .as_str()
        .ok_or_else(|| LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field: "__module__",
            detail: "expected a string".to_string(),
        })?;

    if module_name != LEGACY_MODULE_NAME {
        return Err(LegacySnapshotError::UnexpectedModule {
            path: path.to_string(),
            expected: LEGACY_MODULE_NAME,
            found: module_name.to_string(),
        });
    }

    Ok(())
}

fn get_required_string(
    object: &Map<String, Value>,
    path: &str,
    field: &'static str,
) -> Result<String, LegacySnapshotError> {
    object
        .get(field)
        .ok_or_else(|| LegacySnapshotError::MissingField {
            path: path.to_string(),
            field,
        })?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: "expected a string".to_string(),
        })
}

fn get_optional_string(
    object: &Map<String, Value>,
    path: &str,
    field: &'static str,
) -> Result<Option<String>, LegacySnapshotError> {
    match object.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|inner| Some(inner.to_string()))
            .ok_or_else(|| LegacySnapshotError::InvalidFieldType {
                path: path.to_string(),
                field,
                detail: "expected a string".to_string(),
            }),
    }
}

fn get_optional_bool(
    object: &Map<String, Value>,
    path: &str,
    field: &'static str,
) -> Result<Option<bool>, LegacySnapshotError> {
    match object.get(field) {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(Value::Number(value)) => match value.as_u64() {
            Some(0) => Ok(Some(false)),
            Some(1) => Ok(Some(true)),
            _ => Err(LegacySnapshotError::InvalidFieldType {
                path: path.to_string(),
                field,
                detail: "expected a boolean or 0/1 legacy flag".to_string(),
            }),
        },
        Some(_) => Err(LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: "expected a boolean or 0/1 legacy flag".to_string(),
        }),
    }
}

fn get_required_u32(
    object: &Map<String, Value>,
    path: &str,
    field: &'static str,
) -> Result<u32, LegacySnapshotError> {
    let value = object
        .get(field)
        .ok_or_else(|| LegacySnapshotError::MissingField {
            path: path.to_string(),
            field,
        })?;
    value_to_u32(value, path, field, "expected a non-negative integer")
}

fn get_optional_size(
    object: &Map<String, Value>,
    path: &str,
    field: &'static str,
) -> Result<Size, LegacySnapshotError> {
    match object.get(field) {
        None => Ok(Size::default()),
        Some(Value::Array(items)) => parse_size(items, path, field),
        Some(_) => Err(LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: "expected an array".to_string(),
        }),
    }
}

fn parse_size(
    items: &[Value],
    path: &str,
    field: &'static str,
) -> Result<Size, LegacySnapshotError> {
    match items {
        [] => Ok(Size::empty()),
        [width, height] => Ok(Size::new(
            value_to_u32(
                width,
                path,
                field,
                "expected width as a non-negative integer",
            )?,
            value_to_u32(
                height,
                path,
                field,
                "expected height as a non-negative integer",
            )?,
        )),
        _ => Err(LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: format!(
                "expected an empty array or exactly two integers, got {} elements",
                items.len()
            ),
        }),
    }
}

fn value_to_u32(
    value: &Value,
    path: &str,
    field: &'static str,
    detail: &str,
) -> Result<u32, LegacySnapshotError> {
    let Some(number) = value.as_u64() else {
        return Err(LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: detail.to_string(),
        });
    };

    u32::try_from(number).map_err(|_| LegacySnapshotError::InvalidFieldType {
        path: path.to_string(),
        field,
        detail: "value exceeds u32 range".to_string(),
    })
}

fn parse_sessions(
    value: Option<&Value>,
    path: &str,
    field: &'static str,
) -> Result<Vec<Session>, LegacySnapshotError> {
    parse_nested_array(value, path, field, parse_session)
}

fn parse_windows(
    value: Option<&Value>,
    path: &str,
    field: &'static str,
) -> Result<Vec<Window>, LegacySnapshotError> {
    parse_nested_array(value, path, field, parse_window)
}

fn parse_panes(
    value: Option<&Value>,
    path: &str,
    field: &'static str,
) -> Result<Vec<Pane>, LegacySnapshotError> {
    parse_nested_array(value, path, field, parse_pane)
}

fn parse_nested_array<T, F>(
    value: Option<&Value>,
    path: &str,
    field: &'static str,
    parser: F,
) -> Result<Vec<T>, LegacySnapshotError>
where
    F: Fn(&Value, &str) -> Result<T, LegacySnapshotError>,
{
    let Some(value) = value else {
        return Ok(Vec::new());
    };

    let items = value
        .as_array()
        .ok_or_else(|| LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: "expected an array".to_string(),
        })?;

    items
        .iter()
        .enumerate()
        .map(|(index, item)| parser(item, &format!("{path}.{field}[{index}]")))
        .collect()
}

fn tmux_to_value(tmux: &Tmux) -> Value {
    let mut object = Map::new();
    object.insert("__class__".to_string(), Value::String("Tmux".to_string()));
    object.insert(
        "__module__".to_string(),
        Value::String(LEGACY_MODULE_NAME.to_string()),
    );
    object.insert(
        "create_time".to_string(),
        Value::String(tmux.create_time.clone()),
    );
    object.insert(
        "sessions".to_string(),
        Value::Array(tmux.sessions.iter().map(session_to_value).collect()),
    );
    object.insert("tid".to_string(), Value::String(tmux.tid.clone()));
    Value::Object(object)
}

fn session_to_value(session: &Session) -> Value {
    let mut object = Map::new();
    object.insert(
        "__class__".to_string(),
        Value::String("Session".to_string()),
    );
    object.insert(
        "__module__".to_string(),
        Value::String(LEGACY_MODULE_NAME.to_string()),
    );
    object.insert("attached".to_string(), Value::Bool(session.attached));
    object.insert("name".to_string(), Value::String(session.name.clone()));
    object.insert("size".to_string(), size_to_value(session.size));
    object.insert(
        "windows".to_string(),
        Value::Array(session.windows.iter().map(window_to_value).collect()),
    );
    Value::Object(object)
}

fn window_to_value(window: &Window) -> Value {
    let mut object = Map::new();
    object.insert("__class__".to_string(), Value::String("Window".to_string()));
    object.insert(
        "__module__".to_string(),
        Value::String(LEGACY_MODULE_NAME.to_string()),
    );
    object.insert("active".to_string(), Value::Bool(window.active));
    object.insert("layout".to_string(), Value::String(window.layout.clone()));
    object.insert("name".to_string(), Value::String(window.name.clone()));
    object.insert(
        "panes".to_string(),
        Value::Array(window.panes.iter().map(pane_to_value).collect()),
    );
    object.insert(
        "sess_name".to_string(),
        Value::String(window.sess_name.clone()),
    );
    object.insert(
        "win_id".to_string(),
        Value::Number(Number::from(window.win_id)),
    );
    Value::Object(object)
}

fn pane_to_value(pane: &Pane) -> Value {
    let mut object = Map::new();
    object.insert("__class__".to_string(), Value::String("Pane".to_string()));
    object.insert(
        "__module__".to_string(),
        Value::String(LEGACY_MODULE_NAME.to_string()),
    );
    object.insert("active".to_string(), Value::Bool(pane.active));
    object.insert(
        "cont_file".to_string(),
        Value::String(pane.cont_file.clone()),
    );
    object.insert(
        "pane_id".to_string(),
        Value::Number(Number::from(pane.pane_id)),
    );
    object.insert("path".to_string(), Value::String(pane.path.clone()));
    object.insert(
        "sess_name".to_string(),
        Value::String(pane.sess_name.clone()),
    );
    object.insert("size".to_string(), size_to_value(pane.size));
    object.insert(
        "win_id".to_string(),
        Value::Number(Number::from(pane.win_id)),
    );
    Value::Object(object)
}

fn size_to_value(size: Size) -> Value {
    match size.as_tuple() {
        None => Value::Array(Vec::new()),
        Some((width, height)) => Value::Array(vec![
            Value::Number(Number::from(width)),
            Value::Number(Number::from(height)),
        ]),
    }
}
