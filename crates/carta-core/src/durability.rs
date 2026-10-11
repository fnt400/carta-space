//! Platform-appropriate file and directory durability primitives.
//! Regular files must be flushed using a handle opened with write access:
//! Windows rejects FlushFileBuffers on read-only handles.
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

pub(crate) fn sync_file(path: &Path) -> io::Result<()> {
    OpenOptions::new().write(true).open(path)?.sync_all()
}

#[cfg(not(windows))]
pub(crate) fn sync_directory(path: &Path) -> io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(windows)]
pub(crate) fn sync_directory(_path: &Path) -> io::Result<()> {
    // std::fs::File::open cannot open a directory on Windows for FlushFileBuffers.
    // Explicit file fsync remains mandatory. NTFS metadata flush after rename
    // has no equivalent here; do NOT claim Unix directory-fsync guarantees.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_file_is_durably_flushable() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("config");
        std::fs::write(&file, b"test\n").unwrap();
        sync_file(&file).unwrap();
        sync_directory(directory.path()).unwrap();
    }
}
