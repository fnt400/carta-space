use std::path::{Path, PathBuf};

use carta_format::{Timestamp, WorkId, WorkMetadata};

#[derive(Debug, Clone)]
pub struct Work {
    pub(crate) metadata: WorkMetadata,
    pub(crate) path: PathBuf,
}

impl Work {
    pub fn id(&self) -> WorkId {
        self.metadata.id()
    }

    pub fn created(&self) -> Timestamp {
        self.metadata.created()
    }

    pub fn title(&self) -> &str {
        self.metadata.title()
    }

    pub fn documents(&self) -> &[carta_format::DocumentId] {
        self.metadata.documents()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn metadata(&self) -> &WorkMetadata {
        &self.metadata
    }
}
