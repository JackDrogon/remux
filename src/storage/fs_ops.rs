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

pub(crate) fn rename<E>(
    from: &Path,
    to: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
    fs::rename(from, to).map_err(|source| map(to.to_path_buf(), source))
}

pub(crate) fn remove_dir_all<E>(
    path: &Path,
    map: impl FnOnce(PathBuf, io::Error) -> E,
) -> Result<(), E> {
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
