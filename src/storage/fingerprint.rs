use crate::model::{Pane, Session, Tmux, Window};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactFingerprint {
    schema_major: u16,
    schema_minor: u16,
    sessions: Vec<SessionPrint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionPrint {
    name: String,
    windows: Vec<WindowPrint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WindowPrint {
    window_id: u32,
    layout: String,
    panes: Vec<PanePrint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanePrint {
    pane_id: u32,
    root: Option<RootProcessPrint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RootProcessPrint {
    pid: u32,
    name: String,
    argv: Vec<String>,
}

impl CompactFingerprint {
    pub fn from_tmux(tmux: &Tmux, schema_major: u16, schema_minor: u16) -> Self {
        let mut sessions: Vec<SessionPrint> = tmux.sessions.iter().map(session_print).collect();
        sessions.sort_by(|left, right| left.name.cmp(&right.name));
        Self {
            schema_major,
            schema_minor,
            sessions,
        }
    }

    pub(crate) fn is_covered_by(&self, newer: &Self) -> bool {
        self.schema_major == newer.schema_major
            && self.schema_minor == newer.schema_minor
            && sorted_subset(
                &self.sessions,
                &newer.sessions,
                |session| session.name.as_str(),
                SessionPrint::is_covered_by,
            )
    }
}

impl SessionPrint {
    fn is_covered_by(&self, newer: &Self) -> bool {
        sorted_subset(
            &self.windows,
            &newer.windows,
            |window| &window.window_id,
            WindowPrint::is_covered_by,
        )
    }
}

impl WindowPrint {
    fn is_covered_by(&self, newer: &Self) -> bool {
        self.layout == newer.layout
            && sorted_subset(
                &self.panes,
                &newer.panes,
                |pane| &pane.pane_id,
                |older, newer| older.root == newer.root,
            )
    }
}

fn sorted_subset<T, K: Ord + ?Sized>(
    older: &[T],
    newer: &[T],
    key: impl Fn(&T) -> &K,
    covered: impl Fn(&T, &T) -> bool,
) -> bool {
    let mut newer_index = 0;
    for older_item in older {
        let older_key = key(older_item);
        while newer_index < newer.len() && key(&newer[newer_index]) < older_key {
            newer_index += 1;
        }
        let Some(newer_item) = newer.get(newer_index) else {
            return false;
        };
        if key(newer_item) != older_key || !covered(older_item, newer_item) {
            return false;
        }
        newer_index += 1;
    }
    true
}

fn session_print(session: &Session) -> SessionPrint {
    let mut windows: Vec<WindowPrint> = session.windows.iter().map(window_print).collect();
    windows.sort_by_key(|window| window.window_id);
    SessionPrint {
        name: session.name.clone(),
        windows,
    }
}

fn window_print(window: &Window) -> WindowPrint {
    let mut panes: Vec<PanePrint> = window.panes.iter().map(pane_print).collect();
    panes.sort_by_key(|pane| pane.pane_id);
    WindowPrint {
        window_id: window.window_id,
        layout: window.layout.clone(),
        panes,
    }
}

fn pane_print(pane: &Pane) -> PanePrint {
    PanePrint {
        pane_id: pane.pane_id,
        root: pane.command_tree.as_ref().map(|root| RootProcessPrint {
            pid: root.pid,
            name: root.name.clone(),
            argv: root.argv.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Pane, Process, Session, Tmux, Window};

    use super::CompactFingerprint;

    fn sample_tmux(root: Option<Process>) -> Tmux {
        let mut tmux = Tmux::new("20240101_120000");
        let mut session = Session::new("work");
        let mut window = Window::new("work", 1);
        window.name = "editor".to_string();
        window.layout = "1900,120x40,0,0,0".to_string();
        let mut pane = Pane::new("work", 1, 0);
        pane.path = "/tmp/work".to_string();
        pane.command_tree = root;
        window.panes.push(pane);
        session.windows.push(window);
        tmux.sessions.push(session);
        tmux
    }

    fn root(pid: u32, name: &str, argv: &[&str]) -> Process {
        Process {
            name: name.to_string(),
            argv: argv.iter().map(|value| (*value).to_string()).collect(),
            pid,
            foreground: true,
            children: Vec::new(),
        }
    }

    #[test]
    fn ignores_cwd_children_and_foreground() {
        let mut left = root(18421, "zsh", &["-zsh"]);
        left.foreground = true;
        left.children.push(root(18422, "vim", &["vim", "notes"]));

        let mut right = root(18421, "zsh", &["-zsh"]);
        right.foreground = false;
        right
            .children
            .push(root(99999, "cargo", &["cargo", "test"]));

        let mut left_tmux = sample_tmux(Some(left));
        left_tmux.sessions[0].windows[0].panes[0].path = "/tmp/a".to_string();
        let mut right_tmux = sample_tmux(Some(right));
        right_tmux.sessions[0].windows[0].panes[0].path = "/tmp/b".to_string();

        assert_eq!(
            CompactFingerprint::from_tmux(&left_tmux, 1, 1),
            CompactFingerprint::from_tmux(&right_tmux, 1, 1)
        );
    }

    #[test]
    fn compares_schema_topology_and_root_identity() {
        let base = sample_tmux(Some(root(18421, "zsh", &["-zsh"])));
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 0),
            CompactFingerprint::from_tmux(&base, 1, 1)
        );

        let mut session_name = base.clone();
        session_name.sessions[0].name = "other".to_string();
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&session_name, 1, 1)
        );

        let mut window_id = base.clone();
        window_id.sessions[0].windows[0].window_id = 2;
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&window_id, 1, 1)
        );

        let mut pane_id = base.clone();
        pane_id.sessions[0].windows[0].panes[0].pane_id = 1;
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&pane_id, 1, 1)
        );

        let other_pid = sample_tmux(Some(root(18422, "zsh", &["-zsh"])));
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&other_pid, 1, 1)
        );

        let other_name = sample_tmux(Some(root(18421, "bash", &["-zsh"])));
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&other_name, 1, 1)
        );

        let other_argv = sample_tmux(Some(root(18421, "zsh", &["zsh"])));
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&other_argv, 1, 1)
        );
    }

    #[test]
    fn ignores_timestamps_focus_and_sizes() {
        let mut left = sample_tmux(Some(root(18421, "zsh", &["-zsh"])));
        let mut right = sample_tmux(Some(root(18421, "zsh", &["-zsh"])));
        left.create_time = "2024-01-01 12:00:00".to_string();
        right.create_time = "2024-01-01 12:10:00".to_string();
        left.sessions[0].attached = false;
        right.sessions[0].attached = true;
        left.sessions[0].windows[0].name = "zsh".to_string();
        right.sessions[0].windows[0].name = "tig".to_string();
        left.sessions[0].windows[0].active = true;
        right.sessions[0].windows[0].active = false;
        left.sessions[0].windows[0].panes[0].active = true;
        right.sessions[0].windows[0].panes[0].active = false;
        left.sessions[0].size = crate::model::Size::new(80, 24);
        right.sessions[0].size = crate::model::Size::new(120, 40);
        left.sessions[0].windows[0].panes[0].size = crate::model::Size::new(80, 24);
        right.sessions[0].windows[0].panes[0].size = crate::model::Size::new(120, 40);

        assert_eq!(
            CompactFingerprint::from_tmux(&left, 1, 1),
            CompactFingerprint::from_tmux(&right, 1, 1)
        );
    }

    #[test]
    fn compares_layout_and_treats_missing_root_as_empty() {
        let mut left = sample_tmux(None);
        let right = sample_tmux(None);
        assert_eq!(
            CompactFingerprint::from_tmux(&left, 1, 1),
            CompactFingerprint::from_tmux(&right, 1, 1)
        );

        left.sessions[0].windows[0].layout = "other-layout".to_string();
        assert_ne!(
            CompactFingerprint::from_tmux(&left, 1, 1),
            CompactFingerprint::from_tmux(&right, 1, 1)
        );
    }

    fn window_with_panes(session: &str, window_id: u32, layout: &str, pane_ids: &[u32]) -> Window {
        let mut window = Window::new(session, window_id);
        window.layout = layout.to_string();
        window.panes = pane_ids
            .iter()
            .map(|&pane_id| Pane::new(session, window_id, pane_id))
            .collect();
        window
    }

    fn session_with_windows(name: &str, windows: Vec<Window>) -> Session {
        let mut session = Session::new(name);
        session.windows = windows;
        session
    }

    #[test]
    fn listing_order_does_not_change_fingerprint() {
        let mut left = Tmux::new("20240101_120000");
        left.sessions = vec![
            session_with_windows(
                "beta",
                vec![
                    window_with_panes("beta", 2, "layout-b2", &[1, 0]),
                    window_with_panes("beta", 1, "layout-b1", &[0]),
                ],
            ),
            session_with_windows(
                "alpha",
                vec![window_with_panes("alpha", 1, "layout-a1", &[0])],
            ),
        ];

        let mut right = Tmux::new("20240101_120000");
        right.sessions = vec![
            session_with_windows(
                "alpha",
                vec![window_with_panes("alpha", 1, "layout-a1", &[0])],
            ),
            session_with_windows(
                "beta",
                vec![
                    window_with_panes("beta", 1, "layout-b1", &[0]),
                    window_with_panes("beta", 2, "layout-b2", &[0, 1]),
                ],
            ),
        ];

        assert_eq!(
            CompactFingerprint::from_tmux(&left, 1, 1),
            CompactFingerprint::from_tmux(&right, 1, 1)
        );
    }

    #[test]
    fn extra_session_or_window_is_covered() {
        let mut older = Tmux::new("20240101_120000");
        older.sessions = vec![session_with_windows(
            "work",
            vec![window_with_panes("work", 1, "layout-1", &[0])],
        )];
        let older_print = CompactFingerprint::from_tmux(&older, 1, 1);

        let mut extra_session = older.clone();
        extra_session.sessions.push(session_with_windows(
            "extra",
            vec![window_with_panes("extra", 1, "layout-x", &[0])],
        ));
        let extra_session_print = CompactFingerprint::from_tmux(&extra_session, 1, 1);
        assert!(older_print.is_covered_by(&extra_session_print));
        assert!(
            !extra_session_print.is_covered_by(&older_print),
            "closing a session must not cover the older backup"
        );

        let mut extra_window = older.clone();
        extra_window.sessions[0]
            .windows
            .push(window_with_panes("work", 2, "layout-2", &[0]));
        assert!(older_print.is_covered_by(&CompactFingerprint::from_tmux(&extra_window, 1, 1)));
    }

    #[test]
    fn layout_or_schema_mismatch_is_not_covered() {
        let mut older = Tmux::new("20240101_120000");
        older.sessions = vec![session_with_windows(
            "work",
            vec![window_with_panes("work", 1, "layout-1", &[0])],
        )];
        let older_print = CompactFingerprint::from_tmux(&older, 1, 1);

        let mut layout = older.clone();
        layout.sessions[0].windows[0].layout = "layout-other".to_string();
        assert!(!older_print.is_covered_by(&CompactFingerprint::from_tmux(&layout, 1, 1)));

        assert!(!older_print.is_covered_by(&CompactFingerprint::from_tmux(&older, 1, 0)));
    }
}
