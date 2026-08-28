use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

use crate::{Backup as BackupError, Result};

unsafe extern "C" {
    fn geteuid() -> u32;
}

pub fn discover_socket_names() -> Result<Vec<String>> {
    let socket_dir = socket_dir()?;
    let entries = match fs::read_dir(&socket_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(BackupError::SocketDiscovery { source: error }.into()),
    };

    // Opening the socket directory is fatal (`Backup::SocketDiscovery`).
    // Individual entries are best-effort: sockets can vanish between readdir
    // and stat, leftover non-sockets are ignored, and non-UTF8 names cannot be
    // passed to `tmux -L`. Skipping those rows is the discovery contract, not a
    // silent failure of the directory itself.
    let mut socket_names = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_socket() {
                return None;
            }
            entry.file_name().into_string().ok()
        })
        .collect::<Vec<_>>();

    socket_names.sort();
    socket_names.dedup();
    Ok(socket_names)
}

pub fn socket_dir() -> Result<PathBuf> {
    let parent = env::var_os("TMUX_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Ok(parent.join(format!("tmux-{}", effective_uid())))
}

fn effective_uid() -> u32 {
    unsafe { geteuid() }
}
