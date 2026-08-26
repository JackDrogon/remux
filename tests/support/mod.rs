use std::collections::BTreeMap;

use remux::model::{Pane, Session, Size, Tmux, Window};

pub mod tmux_fake;

#[allow(dead_code)]
pub fn single_window_tmux(
    backup_id: &str,
    session_name: &str,
    create_time: &str,
    pane_paths: &[&str],
) -> (Tmux, BTreeMap<String, Vec<u8>>) {
    let mut tmux = Tmux::new(backup_id);
    tmux.create_time = create_time.to_string();

    let mut session = Session::new(session_name);
    session.size = Size::new(120, 40);

    let mut window = Window::new(session_name, 1);
    window.name = "editor".to_string();
    window.active = true;
    window.layout = "1900,120x40,0,0,0".to_string();

    let mut pane_contents = BTreeMap::new();
    for (index, pane_path) in pane_paths.iter().enumerate() {
        let mut pane = Pane::new(session_name, 1, index as u32);
        pane.active = index == 0;
        pane.size = Size::new(120, 40);
        pane.path = (*pane_path).to_string();
        pane_contents.insert(
            pane.pane_target(),
            format!("content for {pane_path}\n").into_bytes(),
        );
        window.panes.push(pane);
    }

    session.windows.push(window);
    tmux.sessions.push(session);
    (tmux, pane_contents)
}
