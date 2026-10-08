//! GUI uses the same device-local session format and Archive identity as TUI.
//! This is not stored in the Archive and is never included in Git sync.
use carta_app::Session;
use carta_core::ArchiveId;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub fn data_root() -> io::Result<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return Ok(dir.join("carta"));
        }
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "HOME is unset")
    })?;
    Ok(PathBuf::from(home).join(".local/share/carta"))
}

pub fn default_archive_path() -> io::Result<PathBuf> {
    Ok(data_root()?.join("archive"))
}

pub fn session_path(root: &Path, id: ArchiveId) -> PathBuf {
    root.join(format!("session-{id}.json"))
}

pub fn load(root: &Path, id: ArchiveId) -> io::Result<Option<Session>> {
    match fs::read(session_path(root, id)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn save(root: &Path, id: ArchiveId, session: &Session) -> io::Result<()> {
    fs::create_dir_all(root)?;
    let path = session_path(root, id);
    let temp = path.with_extension("tmp");
    fs::write(&temp, serde_json::to_vec_pretty(session).map_err(io::Error::other)?)?;
    fs::rename(temp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use carta_core::Volume;

    #[test]
    fn same_session_roundtrips_with_same_archive_id() {
        let root = std::env::temp_dir().join(format!("carta-gui-session-test-{}", ArchiveId::new_v7()));
        let archive_id = ArchiveId::new_v7();
        let value = Session::new(Volume::new(2026, 10).unwrap());
        save(&root, archive_id, &value).unwrap();
        assert_eq!(load(&root, archive_id).unwrap(), Some(value));
        let _ = fs::remove_dir_all(&root);
    }
}
