use carta_core::{ArchiveId, DocumentId, Volume, WorkId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedView {
    #[serde(alias = "Chronological")]
    CreationDate {
        year: u16,
        month: u8,
    },
    ModificationDate,
    Work {
        id: WorkId,
    },
    Search {
        query: String,
        selected: usize,
    },
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HostSettings {
    #[serde(default)]
    pub portable_keyboard_mode: bool,
}

impl Session {
    pub fn new(volume: Volume) -> Self {
        Self {
            view: SavedView::CreationDate {
                year: volume.year(),
                month: volume.month(),
            },
            position: None,
            work_positions: BTreeMap::new(),
            work_mru: Vec::new(),
        }
    }
}

pub fn data_root() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Ok(path.join("carta"));
        }
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is unset and XDG_DATA_HOME is not an absolute path",
        )
    })?;
    Ok(PathBuf::from(home).join(".local/share/carta"))
}

pub fn default_archive_path() -> io::Result<PathBuf> {
    Ok(data_root()?.join("archive"))
}

pub fn current_host_name() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .or_else(|| {
            fs::read_to_string("/etc/hostname")
                .ok()
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "unknown-host".to_owned())
}

fn host_settings_path(root: &Path, host: &str) -> PathBuf {
    let mut safe = String::new();
    let mut separator_pending = false;
    for character in host.trim().chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            if separator_pending && !safe.is_empty() {
                safe.push('-');
            }
            safe.push(character);
            separator_pending = false;
        } else if !safe.is_empty() {
            separator_pending = true;
        }
    }
    while safe.ends_with('-') {
        safe.pop();
    }
    if safe.is_empty() {
        safe.push_str("unknown-host");
    }
    root.join(format!("host-{safe}.json"))
}

pub fn load_host_settings(root: &Path, host: &str) -> io::Result<HostSettings> {
    Ok(read_json(&host_settings_path(root, host))?.unwrap_or_default())
}

pub fn save_host_settings(root: &Path, host: &str, settings: &HostSettings) -> io::Result<()> {
    write_json(&host_settings_path(root, host), settings)
}

pub fn migrate_legacy_state(root: &Path) -> io::Result<()> {
    let Some(legacy) = legacy_state_root() else {
        return Ok(());
    };
    migrate_legacy_state_from(&legacy, root)
}

fn legacy_state_root() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            return Some(path.join("carta-space"));
        }
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/state/carta-space"))
}

fn migrate_legacy_state_from(legacy: &Path, root: &Path) -> io::Result<()> {
    if legacy == root || !legacy.is_dir() {
        return Ok(());
    }

    fs::create_dir_all(root)?;
    for entry in fs::read_dir(legacy)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };

        if name_str.starts_with("session-")
            && (name_str.ends_with(".json") || name_str.ends_with(".tmp"))
        {
            let target = root.join(&name);
            if !target.exists() {
                fs::copy(entry.path(), &target)?;
            }
            fs::remove_file(entry.path())?;
        } else if name_str == "last-archive.json" {
            fs::remove_file(entry.path())?;
        }
    }

    if fs::read_dir(legacy)?.next().is_none() {
        fs::remove_dir(legacy)?;
    }
    Ok(())
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
    fn legacy_chronological_session_name_is_accepted() {
        let json = r#"{"view":{"Chronological":{"year":2026,"month":9}},"position":null}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(
            session.view,
            SavedView::CreationDate {
                year: 2026,
                month: 9
            }
        );
    }

    #[test]
    fn session_round_trips_in_data_root() {
        let dir = tempfile::tempdir().unwrap();
        let id = ArchiveId::new_v7();
        let volume = Volume::new(2026, 9).unwrap();
        let session = Session::new(volume);
        save_session(dir.path(), id, &session).unwrap();
        assert_eq!(load_session(dir.path(), id).unwrap(), Some(session));
    }

    #[test]
    fn host_settings_are_isolated_by_host_name() {
        let dir = tempfile::tempdir().unwrap();
        let enabled = HostSettings {
            portable_keyboard_mode: true,
        };

        save_host_settings(dir.path(), "nixosvm", &enabled).unwrap();

        assert_eq!(
            load_host_settings(dir.path(), "nixosvm").unwrap(),
            enabled
        );
        assert_eq!(
            load_host_settings(dir.path(), "fermi").unwrap(),
            HostSettings::default()
        );
        assert!(dir.path().join("host-nixosvm.json").exists());
        assert!(!dir.path().join("host-fermi.json").exists());
    }

    #[test]
    fn host_settings_filename_is_sanitized() {
        let dir = tempfile::tempdir().unwrap();
        save_host_settings(
            dir.path(),
            "nixos vm / ssh",
            &HostSettings {
                portable_keyboard_mode: true,
            },
        )
        .unwrap();

        assert!(dir.path().join("host-nixos-vm-ssh.json").exists());
    }

    #[test]
    fn legacy_state_migration_moves_sessions_and_discards_last_archive() {
        let temporary = tempfile::tempdir().unwrap();
        let legacy = temporary.path().join("legacy-state");
        let root = temporary.path().join("data");
        fs::create_dir_all(&legacy).unwrap();

        let id = ArchiveId::new_v7();
        let session = Session::new(Volume::new(2026, 9).unwrap());
        save_session(&legacy, id, &session).unwrap();
        fs::write(
            legacy.join("last-archive.json"),
            r#"{"path":"/tmp/obsolete"}"#,
        )
        .unwrap();
        fs::write(legacy.join("session-stale.tmp"), b"stale").unwrap();

        migrate_legacy_state_from(&legacy, &root).unwrap();

        assert_eq!(load_session(&root, id).unwrap(), Some(session));
        assert_eq!(fs::read(root.join("session-stale.tmp")).unwrap(), b"stale");
        assert!(!legacy.join(format!("session-{id}.json")).exists());
        assert!(!legacy.join("last-archive.json").exists());
        assert!(!legacy.exists());
    }
}
