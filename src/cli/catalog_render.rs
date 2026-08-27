use crate::model::Tmux;
use crate::storage::BackupEntry;

use super::ui;

const LIST_WIDTH: usize = 72;
const TREE_SPACE: &str = "        ";

pub fn no_backups_message() -> &'static str {
    "No backups yet.\nCreate one with: remux backup [name]"
}

pub fn render_summary(backups: &[BackupEntry]) -> String {
    let mut lines = Vec::new();
    lines.push("Saved backups".to_string());
    lines.push(repeat_line('='));
    lines.push(format!(
        " {:>2} {:<17} {:<30} {}",
        "No.", "Name", "Sessions", "Created"
    ));
    lines.push(repeat_line('='));

    for (index, entry) in backups.iter().enumerate() {
        let latest_flag = if index == 0 { '*' } else { ' ' };
        lines.push(format!(
            "{latest_flag}{:>2} {}",
            index + 1,
            format_short_info(&entry.snapshot)
        ));
    }

    lines.push(repeat_line('-'));
    lines.push("(*) first backup in this view".to_string());
    lines.join("\n")
}

pub fn render_detail(entry: &BackupEntry) -> String {
    format!(
        "{}\n{}",
        ui::detail_header(&entry.backup_id, false),
        render_detail_body(&entry.snapshot)
    )
}

pub fn render_interactive_detail(entry: &BackupEntry) -> String {
    format!(
        "{}\n{}\n{}",
        ui::detail_header(&entry.backup_id, true),
        render_detail_body(&entry.snapshot),
        repeat_line('<')
    )
}

fn render_detail_body(tmux: &Tmux) -> String {
    let mut lines = vec![format!(
        "{:>LIST_WIDTH$}",
        format!("Backup was created on {}", tmux.create_time)
    )];
    lines.push(format!(
        " Backup─┬─[{}] ({} sessions):",
        tmux.backup_id,
        tmux.sessions.len()
    ));

    for (session_index, session) in tmux.sessions.iter().enumerate() {
        append_session_detail(&mut lines, tmux, session, session_index);
    }

    lines.join("\n")
}

fn append_session_detail(
    lines: &mut Vec<String>,
    tmux: &Tmux,
    session: &crate::model::Session,
    session_index: usize,
) {
    let is_last_session = session_index + 1 == tmux.sessions.len();
    let session_text = format!(
        "─Session─┬─[{}] ({} windows):",
        session.name,
        session.windows.len()
    );
    lines.push(render_tree_line(session_text, &[is_last_session], 1, false));

    for (window_index, window) in session.windows.iter().enumerate() {
        append_window_detail(lines, session, window, is_last_session, window_index);
    }
}

fn append_window_detail(
    lines: &mut Vec<String>,
    session: &crate::model::Session,
    window: &crate::model::Window,
    is_last_session: bool,
    window_index: usize,
) {
    let is_last_window = window_index + 1 == session.windows.len();
    let window_text = format!(
        "─Window─┬─({}) [{}] ({} panes):",
        window.window_id,
        window.name,
        window.panes.len()
    );
    lines.push(render_tree_line(
        window_text,
        &[is_last_session, is_last_window],
        2,
        false,
    ));

    for (pane_index, pane) in window.panes.iter().enumerate() {
        append_pane_detail(
            lines,
            window,
            pane,
            is_last_session,
            is_last_window,
            pane_index,
        );
    }
}

fn append_pane_detail(
    lines: &mut Vec<String>,
    window: &crate::model::Window,
    pane: &crate::model::Pane,
    is_last_session: bool,
    is_last_window: bool,
    pane_index: usize,
) {
    let is_last_pane = pane_index + 1 == window.panes.len();
    let pane_text = format!("─Pane ({}) {}", pane.pane_id, pane.path);
    lines.push(render_tree_line(
        pane_text,
        &[is_last_session, is_last_window, is_last_pane],
        3,
        false,
    ));
}

fn repeat_line(character: char) -> String {
    character.to_string().repeat(LIST_WIDTH)
}

fn render_tree_line(
    text: String,
    ancestor_is_last: &[bool],
    depth: usize,
    placeholder: bool,
) -> String {
    if depth == 0 {
        return text;
    }

    let current_level = depth - 1;
    let mut line = if ancestor_is_last[current_level] {
        let node = if placeholder { ' ' } else { '└' };
        format!("{TREE_SPACE}{node}{text}")
    } else {
        let node = if placeholder { '│' } else { '├' };
        format!("{TREE_SPACE}{node}{text}")
    };

    if current_level == 1 {
        line = format!(" {line}");
    }

    render_tree_line(line, ancestor_is_last, current_level, true)
}

fn format_short_info(tmux: &Tmux) -> String {
    format_short_info_columns(&tmux.backup_id, &session_names(tmux), &tmux.create_time)
}

fn format_short_info_columns(name: &str, sessions: &str, created_on: &str) -> String {
    format!("{name:<17} {sessions:<30} {created_on}")
}

fn session_names(tmux: &Tmux) -> String {
    tmux.sessions
        .iter()
        .map(|session| session.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
