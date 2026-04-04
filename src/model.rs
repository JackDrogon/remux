use std::cmp::Reverse;

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
    pub tid: String,
    pub sessions: Vec<Session>,
    pub create_time: String,
}

impl Tmux {
    pub fn new<T>(tid: T) -> Self
    where
        T: Into<String>,
    {
        Self {
            tid: tid.into(),
            sessions: Vec::new(),
            create_time: String::new(),
        }
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

    pub fn windows_in_reverse(&self) -> Vec<&Window> {
        let mut windows = self.windows.iter().collect::<Vec<_>>();
        windows.sort_by_key(|window| Reverse(window.win_id));
        windows
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub win_id: u32,
    pub name: String,
    pub panes: Vec<Pane>,
    pub active: bool,
    pub sess_name: String,
    pub layout: String,
}

impl Window {
    pub fn new<T>(sess_name: T, win_id: u32) -> Self
    where
        T: Into<String>,
    {
        Self {
            win_id,
            name: format!("win{win_id}"),
            panes: Vec::new(),
            active: false,
            sess_name: sess_name.into(),
            layout: String::new(),
        }
    }

    pub fn min_pane_id(&self) -> Option<u32> {
        self.panes.iter().map(|pane| pane.pane_id).min()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub pane_id: u32,
    pub size: Size,
    pub path: String,
    pub active: bool,
    pub sess_name: String,
    pub win_id: u32,
}

impl Pane {
    pub fn new<T>(sess_name: T, win_id: u32, pane_id: u32) -> Self
    where
        T: Into<String>,
    {
        Self {
            pane_id,
            size: Size::default(),
            path: "~".to_string(),
            active: false,
            sess_name: sess_name.into(),
            win_id,
        }
    }

    pub fn idstr(&self) -> String {
        format!("{}:{}.{}", self.sess_name, self.win_id, self.pane_id)
    }
}
