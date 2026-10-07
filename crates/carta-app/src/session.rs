use carta_core::{DocumentId, Volume, WorkId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SavedView {
    #[serde(alias = "Chronological")]
    CreationDate { year: u16, month: u8 },
    ModificationDate,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_chronological_session_name_is_accepted() {
        let json = r#"{"view":{"Chronological":{"year":2026,"month":9}},"position":null}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(
            session.view,
            SavedView::CreationDate { year: 2026, month: 9 }
        );
    }

    #[test]
    fn new_session_starts_in_requested_creation_volume() {
        let session = Session::new(Volume::new(2026, 9).unwrap());
        assert_eq!(
            session.view,
            SavedView::CreationDate { year: 2026, month: 9 }
        );
        assert!(session.position.is_none());
        assert!(session.work_positions.is_empty());
        assert!(session.work_mru.is_empty());
    }
}
