use std::collections::HashSet;
use std::fs;

use crate::model::Process;

/// A Linux process id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProcessId(u32);

/// A Linux process-group id. This is not a process id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ProcessGroupId(u32);

/// Foreground process group of a controlling terminal.
///
/// Linux `tpgid` is this value. It is `-1` when the process has no
/// controlling terminal. That absence is data, not a parse failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalForeground {
    Group(ProcessGroupId),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessRecord {
    pid: ProcessId,
    pgrp: ProcessGroupId,
    comm: String,
    argv: Vec<String>,
    child_pids: Vec<ProcessId>,
    terminal_foreground: TerminalForeground,
}

/// Snapshot the pane process tree and mark at most one foreground node.
///
/// `pane_pid` is tmux's first process in the pane, usually the shell.
/// The terminal's foreground job is a process group (`tpgid`), so this:
/// 1. Walks the pane's `/proc` descendants and keeps every readable child.
/// 2. Takes the root process's already-read `tpgid` as the only group signal.
/// 3. Marks a node `foreground` only if it belongs to that group, or, when there is no terminal
///    group, if tmux's current-command name matches a node. It never treats a process-group id as a
///    pid, and it never invents a process when the root is gone.
pub(crate) fn inspect_command_tree(pane_pid: u32, current_command: &str) -> Option<Process> {
    let processes = collect_process_tree(ProcessId(pane_pid));
    let root = processes.iter().find(|process| process.pid.0 == pane_pid)?;
    let selected = select_foreground_process(root.terminal_foreground, current_command, &processes);
    assemble_process_tree(
        ProcessId(pane_pid),
        selected.map(|process| process.pid),
        &processes,
    )
}

fn assemble_process_tree(
    root_pid: ProcessId,
    foreground_pid: Option<ProcessId>,
    processes: &[ProcessRecord],
) -> Option<Process> {
    fn assemble(
        pid: ProcessId,
        foreground_pid: Option<ProcessId>,
        processes: &[ProcessRecord],
        seen: &mut HashSet<ProcessId>,
    ) -> Option<Process> {
        if !seen.insert(pid) {
            return None;
        }
        let record = processes.iter().find(|process| process.pid == pid)?;
        let children = record
            .child_pids
            .iter()
            .filter_map(|child_pid| assemble(*child_pid, foreground_pid, processes, seen))
            .collect();
        Some(Process {
            name: record.comm.clone(),
            argv: record.argv.clone(),
            pid: record.pid.0,
            foreground: foreground_pid == Some(record.pid),
            children,
        })
    }

    assemble(root_pid, foreground_pid, processes, &mut HashSet::new())
}

fn select_foreground_process<'a>(
    terminal_foreground: TerminalForeground,
    current_command: &str,
    processes: &'a [ProcessRecord],
) -> Option<&'a ProcessRecord> {
    match terminal_foreground {
        TerminalForeground::Group(group) => {
            select_from_foreground_group(group, current_command, processes)
        }
        TerminalForeground::None => select_by_current_command_name(current_command, processes),
    }
}

fn select_from_foreground_group<'a>(
    group: ProcessGroupId,
    current_command: &str,
    processes: &'a [ProcessRecord],
) -> Option<&'a ProcessRecord> {
    let members = processes
        .iter()
        .filter(|process| process.pgrp == group)
        .collect::<Vec<_>>();
    if members.is_empty() {
        return None;
    }
    if let Some(process) = named_process(current_command, &members) {
        return Some(process);
    }
    members
        .iter()
        .copied()
        .find(|process| process.pid.0 == group.0)
        .or_else(|| members.first().copied())
}

fn select_by_current_command_name<'a>(
    current_command: &str,
    processes: &'a [ProcessRecord],
) -> Option<&'a ProcessRecord> {
    named_process(current_command, &processes.iter().collect::<Vec<_>>())
}

fn named_process<'a>(
    current_command: &str,
    processes: &[&'a ProcessRecord],
) -> Option<&'a ProcessRecord> {
    if current_command.is_empty() {
        return None;
    }
    processes
        .iter()
        .copied()
        .find(|process| process.comm == current_command)
}

fn collect_process_tree(pane_pid: ProcessId) -> Vec<ProcessRecord> {
    let mut processes: Vec<ProcessRecord> = Vec::new();
    let mut pending = vec![pane_pid];
    while let Some(pid) = pending.pop() {
        if processes.iter().any(|process| process.pid == pid) {
            continue;
        }
        if let Some(process) = read_process_record(pid) {
            pending.extend(process.child_pids.iter().copied());
            processes.push(process);
        }
    }
    processes
}

fn read_process_record(pid: ProcessId) -> Option<ProcessRecord> {
    let ids = parse_process_group_ids(&fs::read_to_string(format!("/proc/{}/stat", pid.0)).ok()?)?;
    Some(ProcessRecord {
        pid,
        pgrp: ids.pgrp,
        comm: process_comm(pid).unwrap_or_default(),
        argv: read_command_argv(pid),
        child_pids: child_process_ids(pid),
        terminal_foreground: ids.terminal_foreground,
    })
}

fn parse_process_group_ids(stat: &str) -> Option<ProcessGroupParse> {
    let rest = stat.rsplit_once(')')?.1;
    let mut fields = rest.split_whitespace();
    // After comm: state ppid pgrp session tty_nr tpgid
    fields.next()?;
    fields.next()?;
    let pgrp = fields.next()?.parse().ok()?;
    fields.next()?;
    fields.next()?;
    let tpgid: i64 = fields.next()?.parse().ok()?;
    Some(ProcessGroupParse {
        pgrp: ProcessGroupId(pgrp),
        terminal_foreground: terminal_foreground_from_tpgid(tpgid),
    })
}

fn terminal_foreground_from_tpgid(tpgid: i64) -> TerminalForeground {
    match u32::try_from(tpgid) {
        Ok(pgrp) if pgrp > 0 => TerminalForeground::Group(ProcessGroupId(pgrp)),
        _ => TerminalForeground::None,
    }
}

struct ProcessGroupParse {
    pgrp: ProcessGroupId,
    terminal_foreground: TerminalForeground,
}

fn process_comm(pid: ProcessId) -> Option<String> {
    let comm = fs::read_to_string(format!("/proc/{}/comm", pid.0)).ok()?;
    Some(comm.trim_end_matches('\n').to_string())
}

fn child_process_ids(pid: ProcessId) -> Vec<ProcessId> {
    let Ok(tasks) = fs::read_dir(format!("/proc/{}/task", pid.0)) else {
        return Vec::new();
    };

    let mut children = Vec::new();
    let mut seen = HashSet::new();
    for task in tasks.flatten() {
        let path = format!(
            "/proc/{}/task/{}/children",
            pid.0,
            task.file_name().to_string_lossy()
        );
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for child_pid in parse_child_process_ids(&text) {
            if seen.insert(child_pid) {
                children.push(ProcessId(child_pid));
            }
        }
    }
    children
}

fn parse_child_process_ids(text: &str) -> Vec<u32> {
    text.split_whitespace()
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn read_command_argv(pid: ProcessId) -> Vec<String> {
    let Ok(bytes) = fs::read(format!("/proc/{}/cmdline", pid.0)) else {
        return Vec::new();
    };
    parse_command_argv(&bytes)
}

fn parse_command_argv(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ProcessGroupId, ProcessId, ProcessRecord, TerminalForeground, parse_child_process_ids,
        parse_command_argv, parse_process_group_ids, select_foreground_process,
        terminal_foreground_from_tpgid,
    };

    fn process(pid: u32, pgrp: u32, comm: &str, argv: &[&str]) -> ProcessRecord {
        process_with_children(pid, pgrp, comm, argv, &[])
    }

    fn process_with_children(
        pid: u32,
        pgrp: u32,
        comm: &str,
        argv: &[&str],
        child_pids: &[u32],
    ) -> ProcessRecord {
        ProcessRecord {
            pid: ProcessId(pid),
            pgrp: ProcessGroupId(pgrp),
            comm: comm.to_string(),
            argv: argv.iter().map(|value| (*value).to_string()).collect(),
            child_pids: child_pids.iter().copied().map(ProcessId).collect(),
            terminal_foreground: TerminalForeground::None,
        }
    }

    #[test]
    fn parse_process_group_ids_reads_pgrp_and_tpgid_after_comm() {
        let stat = "1234 (tmux: server) S 1 1234 1234 0 4321 0 0 0 0 0";
        let ids = parse_process_group_ids(stat).expect("stat should parse");
        assert_eq!(ids.pgrp, ProcessGroupId(1234));
        assert_eq!(
            ids.terminal_foreground,
            TerminalForeground::Group(ProcessGroupId(4321))
        );
    }

    #[test]
    fn parse_process_group_ids_ignores_parentheses_inside_comm() {
        let stat = "99 (weird (name)) R 1 99 99 34816 88 4194304";
        let ids = parse_process_group_ids(stat).expect("stat should parse");
        assert_eq!(ids.pgrp, ProcessGroupId(99));
        assert_eq!(
            ids.terminal_foreground,
            TerminalForeground::Group(ProcessGroupId(88))
        );
    }

    #[test]
    fn missing_or_invalid_tpgid_keeps_the_process_and_clears_the_foreground_group() {
        assert_eq!(terminal_foreground_from_tpgid(-1), TerminalForeground::None);
        assert_eq!(terminal_foreground_from_tpgid(0), TerminalForeground::None);

        let stat = "50 (cat) S 1 50 50 0 -1 0 0 0 0 0";
        let ids = parse_process_group_ids(stat).expect("stat without a tty should still parse");
        assert_eq!(ids.pgrp, ProcessGroupId(50));
        assert_eq!(ids.terminal_foreground, TerminalForeground::None);
    }

    #[test]
    fn parse_command_argv_splits_on_nuls() {
        assert_eq!(
            parse_command_argv(b"vim\0/tmp/notes.md\0"),
            vec!["vim".to_string(), "/tmp/notes.md".to_string()]
        );
    }

    #[test]
    fn prefers_foreground_process_when_background_has_the_same_name() {
        let processes = [
            process(10, 10, "zsh", &["zsh"]),
            process(20, 10, "sleep", &["sleep", "999"]),
            process(30, 30, "sleep", &["sleep", "1"]),
        ];

        let selected = select_foreground_process(
            TerminalForeground::Group(ProcessGroupId(30)),
            "sleep",
            &processes,
        )
        .expect("a foreground sleep should be selected");

        assert_eq!(selected.pid, ProcessId(30));
        assert_eq!(selected.argv, vec!["sleep".to_string(), "1".to_string()]);
    }

    #[test]
    fn uses_foreground_group_leader_when_command_name_is_missing() {
        let processes = [
            process(10, 10, "zsh", &["zsh"]),
            process(20, 10, "sleep", &["sleep", "999"]),
            process(30, 30, "vim", &["vim", "notes.md"]),
        ];

        let selected = select_foreground_process(
            TerminalForeground::Group(ProcessGroupId(30)),
            "",
            &processes,
        )
        .expect("the foreground group leader should be selected");

        assert_eq!(selected.pid, ProcessId(30));
        assert_eq!(selected.comm, "vim");
    }

    #[test]
    fn without_a_terminal_group_only_the_named_current_command_is_marked() {
        let processes = [
            process(10, 10, "zsh", &["zsh"]),
            process(20, 10, "vim", &["vim", "notes.md"]),
        ];

        let selected = select_foreground_process(TerminalForeground::None, "vim", &processes)
            .expect("tmux's current-command name should identify the process");
        assert_eq!(selected.pid, ProcessId(20));

        assert!(
            select_foreground_process(TerminalForeground::None, "", &processes).is_none(),
            "without a terminal group or current-command name, no process is foreground"
        );
    }

    #[test]
    fn does_not_treat_background_process_as_foreground_when_group_members_are_missing() {
        let processes = [
            process(10, 10, "zsh", &["zsh"]),
            process(20, 10, "sleep", &["sleep", "999"]),
        ];

        assert!(
            select_foreground_process(
                TerminalForeground::Group(ProcessGroupId(99)),
                "sleep",
                &processes
            )
            .is_none(),
            "a known foreground group with no visible members must not pick a background process"
        );
    }

    #[test]
    fn inspect_command_tree_returns_none_when_root_process_is_gone() {
        assert!(super::inspect_command_tree(u32::MAX, "vim").is_none());
    }

    #[test]
    fn parse_child_process_ids_splits_whitespace() {
        assert_eq!(parse_child_process_ids("20 30\n"), vec![20, 30]);
    }

    #[test]
    fn assemble_process_tree_keeps_background_and_foreground_children() {
        let processes = [
            process_with_children(10, 10, "zsh", &["zsh"], &[20, 30]),
            process(20, 10, "sleep", &["sleep", "999"]),
            process(30, 30, "vim", &["vim", "notes.md"]),
        ];

        let tree = super::assemble_process_tree(ProcessId(10), Some(ProcessId(30)), &processes)
            .expect("tree should build");

        assert_eq!(tree.pid, 10);
        assert_eq!(tree.name, "zsh");
        assert!(!tree.foreground);
        assert_eq!(
            tree.children
                .iter()
                .map(|child| child.pid)
                .collect::<Vec<_>>(),
            vec![20, 30]
        );
        assert!(!tree.children[0].foreground);
        assert!(tree.children[1].foreground);
        assert_eq!(
            tree.children[0].argv,
            vec!["sleep".to_string(), "999".to_string()]
        );
        assert_eq!(tree.children[1].name, "vim");
    }
}
