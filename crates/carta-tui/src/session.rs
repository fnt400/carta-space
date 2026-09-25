use carta_core::{ArchiveId, DocumentId, Volume, WorkId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedView {
    Chronological { year: u16, month: u8 },
    Work { id: WorkId },
    Search { query: String, selected: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Position {
    pub document: DocumentId,
    pub byte: usize,
    pub scroll: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub view: SavedView,
    pub position: Option<Position>,
    #[serde(default)]
    pub work_positions: BTreeMap<WorkId, Position>,
    #[serde(default)]
    pub work_mru: Vec<WorkId>,
}

impl Session {
    pub fn new(volume: Volume) -> Self {
        Self {
            view: SavedView::Chronological {
                year: volume.year(),
                month: volume.month(),
            },
            position: None,
            work_positions: BTreeMap::new(),
            work_mru: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LastArchive {
    path: PathBuf,
}

pub fn state_root() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("carta-space"));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "HOME and XDG_STATE_HOME are unset")
    })?;
    Ok(PathBuf::from(home).join(".local/state/carta-space"))
}
pub fn load_last_archive(root: &Path) -> io::Result<Option<PathBuf>> {
    read_json(&root.join("last-archive.json")).map(|v: Option<LastArchive>| v.map(|x| x.path))
}
pub fn save_last_archive(root: &Path, path: &Path) -> io::Result<()> {
    write_json(
        &root.join("last-archive.json"),
        &LastArchive {
            path: path.to_path_buf(),
        },
    )
}
pub fn load_session(root: &Path, id: ArchiveId) -> io::Result<Option<Session>> {
    read_json(&root.join(format!("session-{id}.json")))
}
pub fn save_session(root: &Path, id: ArchiveId, session: &Session) -> io::Result<()> {
    write_json(&root.join(format!("session-{id}.json")), session)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> io::Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn write_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension("tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(io::Error::other)?,
    )?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_round_trips_outside_archive() {
        let dir = tempfile::tempdir().unwrap();
        let id = ArchiveId::new_v7();
        let volume = Volume::new(2026, 9).unwrap();
        let session = Session::new(volume);
        save_session(dir.path(), id, &session).unwrap();
        assert_eq!(load_session(dir.path(), id).unwrap(), Some(session));
    }
}
