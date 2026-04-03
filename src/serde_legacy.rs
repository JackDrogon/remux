use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use serde::Deserialize;
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

enum FastParseError {
    Legacy(LegacySnapshotError),
    Serde,
}

#[derive(Deserialize)]
struct TypedLegacySummaryTmux {
    #[serde(rename = "__class__")]
    legacy_class: Option<String>,
    #[serde(rename = "__module__")]
    legacy_module: Option<String>,
    tid: String,
    #[serde(default)]
    create_time: String,
    #[serde(default)]
    sessions: Vec<TypedLegacySummarySession>,
}

#[derive(Deserialize)]
struct TypedLegacySummarySession {
    #[serde(rename = "__class__")]
    legacy_class: Option<String>,
    #[serde(rename = "__module__")]
    legacy_module: Option<String>,
    name: String,
}

#[derive(Deserialize)]
struct TypedLegacyTmux {
    #[serde(rename = "__class__")]
    legacy_class: Option<String>,
    #[serde(rename = "__module__")]
    legacy_module: Option<String>,
    tid: String,
    #[serde(default)]
    create_time: String,
    #[serde(default)]
    sessions: Vec<TypedLegacySession>,
}

#[derive(Deserialize)]
struct TypedLegacySession {
    #[serde(rename = "__class__")]
    legacy_class: Option<String>,
    #[serde(rename = "__module__")]
    legacy_module: Option<String>,
    name: String,
    attached: Option<LegacyBoolValue>,
    #[serde(default)]
    size: Vec<u32>,
    #[serde(default)]
    windows: Vec<TypedLegacyWindow>,
}

#[derive(Deserialize)]
struct TypedLegacyWindow {
    #[serde(rename = "__class__")]
    legacy_class: Option<String>,
    #[serde(rename = "__module__")]
    legacy_module: Option<String>,
    win_id: u32,
    sess_name: String,
    name: Option<String>,
    active: Option<LegacyBoolValue>,
    layout: Option<String>,
    #[serde(default)]
    panes: Vec<TypedLegacyPane>,
}

#[derive(Deserialize)]
struct TypedLegacyPane {
    #[serde(rename = "__class__")]
    legacy_class: Option<String>,
    #[serde(rename = "__module__")]
    legacy_module: Option<String>,
    pane_id: u32,
    win_id: u32,
    sess_name: String,
    #[serde(default)]
    size: Vec<u32>,
    path: Option<String>,
    active: Option<LegacyBoolValue>,
    cont_file: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LegacyBoolValue {
    Bool(bool),
    Number(u8),
}

pub fn from_reader<R>(reader: R) -> Result<Tmux, LegacySnapshotError>
where
    R: Read,
{
    let bytes = read_all_bytes(reader)?;
    from_bytes(&bytes)
}

pub fn from_str(input: &str) -> Result<Tmux, LegacySnapshotError> {
    from_bytes(input.as_bytes())
}

pub fn read_snapshot_file<P>(path: P) -> Result<Tmux, LegacySnapshotError>
where
    P: AsRef<Path>,
{
    let file = File::open(path)?;
    from_reader(file)
}

pub fn read_snapshot_summary_file<P>(path: P) -> Result<Tmux, LegacySnapshotError>
where
    P: AsRef<Path>,
{
    let file = File::open(path)?;
    let bytes = read_all_bytes(file)?;
    summary_from_bytes(&bytes)
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

fn read_all_bytes<R>(mut reader: R) -> Result<Vec<u8>, LegacySnapshotError>
where
    R: Read,
{
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn from_bytes(bytes: &[u8]) -> Result<Tmux, LegacySnapshotError> {
    match try_parse_full_fast(bytes) {
        Ok(tmux) => Ok(tmux),
        Err(FastParseError::Legacy(error)) => Err(error),
        Err(FastParseError::Serde) => parse_tmux_from_value_bytes(bytes),
    }
}

fn summary_from_bytes(bytes: &[u8]) -> Result<Tmux, LegacySnapshotError> {
    match try_parse_summary_fast(bytes) {
        Ok(tmux) => Ok(tmux),
        Err(FastParseError::Legacy(error)) => Err(error),
        Err(FastParseError::Serde) => summarize_tmux(&from_bytes(bytes)?),
    }
}

fn parse_tmux_from_value_bytes(bytes: &[u8]) -> Result<Tmux, LegacySnapshotError> {
    let value: Value = serde_json::from_slice(bytes)?;
    parse_tmux(&value, "$")
}

fn try_parse_summary_fast(bytes: &[u8]) -> Result<Tmux, FastParseError> {
    let tmux: TypedLegacySummaryTmux =
        serde_json::from_slice(bytes).map_err(|_| FastParseError::Serde)?;
    tmux.into_model().map_err(FastParseError::Legacy)
}

fn try_parse_full_fast(bytes: &[u8]) -> Result<Tmux, FastParseError> {
    let tmux: TypedLegacyTmux = serde_json::from_slice(bytes).map_err(|_| FastParseError::Serde)?;
    tmux.into_model().map_err(FastParseError::Legacy)
}

fn summarize_tmux(tmux: &Tmux) -> Result<Tmux, LegacySnapshotError> {
    let mut summary = Tmux::new(tmux.tid.clone());
    summary.create_time = tmux.create_time.clone();
    summary.sessions = tmux
        .sessions
        .iter()
        .map(|session| Ok(Session::new(session.name.clone())))
        .collect::<Result<Vec<_>, LegacySnapshotError>>()?;
    Ok(summary)
}

impl TypedLegacySummaryTmux {
    fn into_model(self) -> Result<Tmux, LegacySnapshotError> {
        validate_markers_fast(&self.legacy_class, &self.legacy_module, "$", "Tmux")?;

        let mut tmux = Tmux::new(self.tid);
        tmux.create_time = self.create_time;
        tmux.sessions = self
            .sessions
            .into_iter()
            .enumerate()
            .map(|(index, session)| session.into_model(&format!("$.sessions[{index}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tmux)
    }
}

impl TypedLegacySummarySession {
    fn into_model(self, path: &str) -> Result<Session, LegacySnapshotError> {
        validate_markers_fast(&self.legacy_class, &self.legacy_module, path, "Session")?;
        Ok(Session::new(self.name))
    }
}

impl TypedLegacyTmux {
    fn into_model(self) -> Result<Tmux, LegacySnapshotError> {
        validate_markers_fast(&self.legacy_class, &self.legacy_module, "$", "Tmux")?;

        let mut tmux = Tmux::new(self.tid);
        tmux.create_time = self.create_time;
        tmux.sessions = self
            .sessions
            .into_iter()
            .enumerate()
            .map(|(index, session)| session.into_model(&format!("$.sessions[{index}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(tmux)
    }
}

impl TypedLegacySession {
    fn into_model(self, path: &str) -> Result<Session, LegacySnapshotError> {
        validate_markers_fast(&self.legacy_class, &self.legacy_module, path, "Session")?;

        let mut session = Session::new(self.name);
        session.attached =
            parse_fast_optional_bool(self.attached, path, "attached")?.unwrap_or(false);
        session.size = parse_fast_size(self.size, path, "size")?;
        session.windows = self
            .windows
            .into_iter()
            .enumerate()
            .map(|(index, window)| window.into_model(&format!("{path}.windows[{index}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(session)
    }
}

impl TypedLegacyWindow {
    fn into_model(self, path: &str) -> Result<Window, LegacySnapshotError> {
        validate_markers_fast(&self.legacy_class, &self.legacy_module, path, "Window")?;

        let mut window = Window::new(self.sess_name, self.win_id);
        if let Some(name) = self.name {
            window.name = name;
        }
        window.active = parse_fast_optional_bool(self.active, path, "active")?.unwrap_or(false);
        window.layout = self.layout.unwrap_or_default();
        window.panes = self
            .panes
            .into_iter()
            .enumerate()
            .map(|(index, pane)| pane.into_model(&format!("{path}.panes[{index}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(window)
    }
}

impl TypedLegacyPane {
    fn into_model(self, path: &str) -> Result<Pane, LegacySnapshotError> {
        validate_markers_fast(&self.legacy_class, &self.legacy_module, path, "Pane")?;

        let mut pane = Pane::new(self.sess_name, self.win_id, self.pane_id);
        pane.size = parse_fast_size(self.size, path, "size")?;
        pane.path = self.path.unwrap_or_else(|| "~".to_string());
        pane.active = parse_fast_optional_bool(self.active, path, "active")?.unwrap_or(false);
        pane.cont_file = self.cont_file.unwrap_or_default();
        Ok(pane)
    }
}

fn validate_markers_fast(
    class_name: &Option<String>,
    module_name: &Option<String>,
    path: &str,
    expected_class: &'static str,
) -> Result<(), LegacySnapshotError> {
    let class_name = class_name
        .as_deref()
        .ok_or_else(|| LegacySnapshotError::MissingMarker {
            path: path.to_string(),
            marker: "__class__",
        })?;

    if class_name != expected_class {
        return Err(LegacySnapshotError::UnexpectedClass {
            path: path.to_string(),
            expected: expected_class,
            found: class_name.to_string(),
        });
    }

    let module_name = module_name
        .as_deref()
        .ok_or_else(|| LegacySnapshotError::MissingMarker {
            path: path.to_string(),
            marker: "__module__",
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

fn parse_fast_optional_bool(
    value: Option<LegacyBoolValue>,
    path: &str,
    field: &'static str,
) -> Result<Option<bool>, LegacySnapshotError> {
    match value {
        None => Ok(None),
        Some(LegacyBoolValue::Bool(value)) => Ok(Some(value)),
        Some(LegacyBoolValue::Number(0)) => Ok(Some(false)),
        Some(LegacyBoolValue::Number(1)) => Ok(Some(true)),
        Some(LegacyBoolValue::Number(_)) => Err(LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: "expected a boolean or 0/1 legacy flag".to_string(),
        }),
    }
}

fn parse_fast_size(
    size: Vec<u32>,
    path: &str,
    field: &'static str,
) -> Result<Size, LegacySnapshotError> {
    match size.as_slice() {
        [] => Ok(Size::empty()),
        [width, height] => Ok(Size::new(*width, *height)),
        _ => Err(LegacySnapshotError::InvalidFieldType {
            path: path.to_string(),
            field,
            detail: format!(
                "expected an empty array or exactly two integers, got {} elements",
                size.len()
            ),
        }),
    }
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
