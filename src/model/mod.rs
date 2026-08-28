use std::cmp::Reverse;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size(Option<(u32, u32)>);

impl Size {
    pub fn empty() -> Self {
        Self(None)
    }

    pub fn new(width: u32, height: u32) -> Self {
        Self(Some((width, height)))
    }

    pub fn as_tuple(&self) -> Option<(u32, u32)> {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

impl From<(u32, u32)> for Size {
    fn from(value: (u32, u32)) -> Self {
        Self::new(value.0, value.1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tmux {
    pub backup_id: String,
    pub sessions: Vec<Session>,
    pub create_time: String,
}

impl Tmux {
    pub fn new<T>(backup_id: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            backup_id: backup_id.into(),
            sessions: Vec::new(),
            create_time: String::new(),
        }
    }

    pub fn panes(&self) -> impl Iterator<Item = &Pane> {
        self.sessions.iter().flat_map(|session| {
            session
                .windows
                .iter()
                .flat_map(|window| window.panes.iter())
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub name: String,
    pub attached: bool,
    pub size: Size,
    pub windows: Vec<Window>,
}

impl Session {
    pub fn new<T>(name: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            name: name.into(),
            attached: false,
            size: Size::default(),
            windows: Vec::new(),
        }
    }

    pub fn windows_in_restore_order(&self) -> Vec<&Window> {
        let mut windows = self.windows.iter().collect::<Vec<_>>();
        windows.sort_by_key(|window| Reverse(window.window_id));
        windows
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub window_id: u32,
    pub name: String,
    pub panes: Vec<Pane>,
    pub active: bool,
    pub session_name: String,
    pub layout: String,
}

impl Window {
    pub fn new<T>(session_name: T, window_id: u32) -> Self
    where
        T: Into<String>,
    {
        Self {
            window_id,
            name: format!("win{window_id}"),
            panes: Vec::new(),
            active: false,
            session_name: session_name.into(),
            layout: String::new(),
        }
    }

    pub fn min_pane_id(&self) -> Option<u32> {
        self.panes.iter().map(|pane| pane.pane_id).min()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: String,
    pub argv: Vec<String>,
    pub pid: u32,
    pub foreground: bool,
    pub children: Vec<Process>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub pane_id: u32,
    pub size: Size,
    pub path: String,
    pub active: bool,
    pub session_name: String,
    pub window_id: u32,
    pub command_tree: Option<Process>,
}

impl Pane {
    pub fn new<T>(session_name: T, window_id: u32, pane_id: u32) -> Self
    where
        T: Into<String>,
    {
        Self {
            pane_id,
            size: Size::default(),
            path: "~".to_string(),
            active: false,
            session_name: session_name.into(),
            window_id,
            command_tree: None,
        }
    }

    pub fn pane_target(&self) -> PaneTarget {
        PaneTarget::from_parts(&self.session_name, self.window_id, self.pane_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowTarget(String);

impl WindowTarget {
    pub fn from_parts(session_name: &str, window_id: impl fmt::Display) -> Self {
        Self(format!("{session_name}:{window_id}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for WindowTarget {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for WindowTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaneTarget(String);

impl PaneTarget {
    pub fn from_parts(
        session_name: &str,
        window_id: impl fmt::Display,
        pane_id: impl fmt::Display,
    ) -> Self {
        Self(format!("{session_name}:{window_id}.{pane_id}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for PaneTarget {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PaneTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
