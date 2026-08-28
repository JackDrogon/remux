use std::fs::{self, DirEntry, File, Metadata, ReadDir};
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn create_dir_all<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
    fs::create_dir_all(path).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn write_bytes<E>(
    path: &Path,
    bytes: impl AsRef<[u8]>,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
    fs::write(path, bytes).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn write_string<E>(
    path: &Path,
    content: &str,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
    fs::write(path, content).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn read_bytes<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<Vec<u8>, E> {
    fs::read(path).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn read_to_string<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<String, E> {
    fs::read_to_string(path).map_err(|source| map(path.to_path_buf(), source))
}

/// Publish `from` onto `to` only if `to` does not exist (any directory entry).
///
/// Linux `rename(2)` of a directory replaces an empty destination. Backup-id
/// occupancy forbids that, so this uses `renameat2(..., RENAME_NOREPLACE)`.
/// Failure stays persist I/O; callers do not reclassify errno.
pub(crate) fn rename_noreplace<E>(
    from: &Path,
    to: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
    rename_dir_noreplace(from, to).map_err(|source| map(to.to_path_buf(), source))
}

fn rename_dir_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    let from_c = path_to_cstring(from)?;
    let to_c = path_to_cstring(to)?;
    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn path_to_cstring(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains interior nul"))
}

pub(crate) fn remove_dir_all<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
    #[cfg(test)]
    if let Some(source) = inject::remove_dir_all_error() {
        return Err(map(path.to_path_buf(), source));
    }
    fs::remove_dir_all(path).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn read_dir<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<ReadDir, E> {
    fs::read_dir(path).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn metadata<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<Metadata, E> {
    fs::metadata(path).map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn optional_metadata<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<Option<Metadata>, E> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(map(path.to_path_buf(), source)),
    }
}

/// lstat: a dangling symlink still occupies the name.
pub(crate) fn optional_symlink_metadata<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<Option<Metadata>, E> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(map(path.to_path_buf(), source)),
    }
}

pub(crate) fn dir_entry<E>(
    entry: Result<DirEntry, io::Error>,
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<DirEntry, E> {
    entry.map_err(|source| map(path.to_path_buf(), source))
}

pub(crate) fn sync_file<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E + Copy,
) -> Result<(), E> {
    let file = File::open(path).map_err(|source| map(path.to_path_buf(), source))?;
    file.sync_all()
        .map_err(|source| map(path.to_path_buf(), source))
}

#[cfg(unix)]
pub(crate) fn sync_dir<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E + Copy,
) -> Result<(), E> {
    #[cfg(test)]
    if let Some(source) = inject::sync_dir_error(path) {
        return Err(map(path.to_path_buf(), source));
    }
    let file = File::open(path).map_err(|source| map(path.to_path_buf(), source))?;
    file.sync_all()
        .map_err(|source| map(path.to_path_buf(), source))
}

#[cfg(not(unix))]
pub(crate) fn sync_dir<E>(
    _path: &Path,
    _map: impl FnOnce(PathBuf, io::Error) -> E + Copy,
) -> Result<(), E> {
    Ok(())
}

#[cfg(test)]
pub(crate) mod inject {
    use std::cell::RefCell;
    use std::io;
    use std::path::{Path, PathBuf};

    #[derive(Default)]
    struct Plan {
        fail_sync_dir: Option<PathBuf>,
        fail_remove_dir_all: bool,
    }

    thread_local! {
        static PLAN: RefCell<Plan> = RefCell::new(Plan::default());
    }

    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            PLAN.with(|plan| *plan.borrow_mut() = Plan::default());
        }
    }

    pub fn fail_sync_dir(path: impl Into<PathBuf>) -> Guard {
        PLAN.with(|plan| plan.borrow_mut().fail_sync_dir = Some(path.into()));
        Guard
    }

    pub fn fail_remove_dir_all() -> Guard {
        PLAN.with(|plan| plan.borrow_mut().fail_remove_dir_all = true);
        Guard
    }

    pub(super) fn sync_dir_error(path: &Path) -> Option<io::Error> {
        PLAN.with(|plan| {
            let mut plan = plan.borrow_mut();
            if plan
                .fail_sync_dir
                .as_ref()
                .is_some_and(|wanted| wanted == path)
            {
                plan.fail_sync_dir = None;
                Some(io::Error::other("injected sync_dir failure"))
            } else {
                None
            }
        })
    }

    pub(super) fn remove_dir_all_error() -> Option<io::Error> {
        PLAN.with(|plan| {
            let mut plan = plan.borrow_mut();
            if !plan.fail_remove_dir_all {
                return None;
            }
            plan.fail_remove_dir_all = false;
            Some(io::Error::other("injected remove_dir_all failure"))
        })
    }
}
