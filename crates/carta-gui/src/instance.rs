//! Exclusive per-user process lock. The OS releases it even after a crash.
//! The lock file is device-local and outside the synchronized Archive.
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io;

pub fn acquire() -> io::Result<Option<File>> {
    let root = crate::session::data_root()?;
    fs::create_dir_all(&root)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("carta-gui.lock"))?;
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error),
    }
}
