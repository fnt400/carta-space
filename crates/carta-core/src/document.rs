use std::path::{Path, PathBuf};

use carta_format::{DocumentId, DocumentMetadata, Timestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Volume {
    year: u16,
    month: u8,
}

impl Volume {
    pub fn new(year: u16, month: u8) -> Option<Self> {
        (year <= 9999 && (1..=12).contains(&month)).then_some(Self { year, month })
    }

    pub(crate) fn from_timestamp(timestamp: Timestamp) -> Option<Self> {
        let year = u16::try_from(timestamp.year()).ok()?;
        let month = u8::try_from(timestamp.month()).ok()?;
        if year > 9999 || !(1..=12).contains(&month) {
            return None;
        }
        Some(Self { year, month })
    }

    pub(crate) fn from_parts(year: u16, month: u8) -> Self {
        Self { year, month }
    }

    pub fn year(self) -> u16 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }
}

#[derive(Debug, Clone)]
pub struct DocumentInfo {
    pub(crate) metadata: DocumentMetadata,
    pub(crate) volume: Volume,
    pub(crate) path: PathBuf,
    pub(crate) metadata_bytes: Vec<u8>,
    pub(crate) content_bytes: Vec<u8>,
}

impl DocumentInfo {
    pub fn id(&self) -> DocumentId {
        self.metadata.id()
    }

    pub fn created(&self) -> Timestamp {
        self.metadata.created()
    }

    pub fn modified(&self) -> Timestamp {
        self.metadata.modified()
    }

    pub fn volume(&self) -> Volume {
        self.volume
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    pub fn locked(&self) -> bool {
        self.metadata.locked()
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    metadata: DocumentMetadata,
    content: String,
}

impl Document {
    pub(crate) fn new(metadata: DocumentMetadata, content: String) -> Self {
        Self { metadata, content }
    }

    pub fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn modified(&self) -> Timestamp {
        self.metadata.modified()
    }

    pub fn locked(&self) -> bool {
        self.metadata.locked()
    }

    /// Returns the human-facing label defined by the v0.1 interaction contract.
    pub fn derived_label(&self) -> String {
        crate::content::document_label(&self.content, self.metadata.created())
    }
}
