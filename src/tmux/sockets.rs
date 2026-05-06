use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

unsafe extern "C" {
    fn geteuid() -> u32;
}

pub fn discover_socket_names() -> Result<Vec<String>, io::Error> {
    let socket_dir = socket_dir()?;
    let entries = match fs::read_dir(&socket_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut socket_names = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() && !file_type.is_socket() {
                return None;
            }
            entry.file_name().into_string().ok()
        })
        .collect::<Vec<_>>();

    socket_names.sort();
    socket_names.dedup();
    Ok(socket_names)
}

pub fn socket_dir() -> Result<PathBuf, io::Error> {
    let parent = env::var_os("TMUX_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Ok(parent.join(format!("tmux-{}", effective_uid())))
}

fn effective_uid() -> u32 {
    unsafe { geteuid() }
}
