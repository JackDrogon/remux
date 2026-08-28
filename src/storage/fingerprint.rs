use crate::model::{Pane, Tmux};

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
    name: String,
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
        Self {
            schema_major,
            schema_minor,
            sessions: tmux
                .sessions
                .iter()
                .map(|session| SessionPrint {
                    name: session.name.clone(),
                    windows: session
                        .windows
                        .iter()
                        .map(|window| WindowPrint {
                            window_id: window.window_id,
                            name: window.name.clone(),
                            layout: window.layout.clone(),
                            panes: window.panes.iter().map(pane_print).collect(),
                        })
                        .collect(),
                })
                .collect(),
        }
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

        let mut window_name = base.clone();
        window_name.sessions[0].windows[0].name = "shell".to_string();
        assert_ne!(
            CompactFingerprint::from_tmux(&base, 1, 1),
            CompactFingerprint::from_tmux(&window_name, 1, 1)
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
}
