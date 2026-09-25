use std::path::{Path, PathBuf};

use carta_format::{DocumentId, Timestamp, WorkId, WorkMetadata};

#[derive(Debug, Clone)]
pub struct Work {
    pub(crate) metadata: WorkMetadata,
    pub(crate) path: PathBuf,
    pub(crate) metadata_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkProjection {
    work: WorkId,
    items: Vec<WorkProjectionItem>,
}

impl WorkProjection {
    pub(crate) fn new(work: WorkId, documents: &[DocumentId]) -> Self {
        let mut items = Vec::with_capacity(documents.len().saturating_mul(2).saturating_sub(1));
        for (index, document) in documents.iter().copied().enumerate() {
            if index != 0 {
                items.push(WorkProjectionItem::Boundary {
                    before: documents[index - 1],
                    after: document,
                });
            }
            items.push(WorkProjectionItem::Document(document));
        }
        Self { work, items }
    }

    pub fn work(&self) -> WorkId {
        self.work
    }

    pub fn items(&self) -> &[WorkProjectionItem] {
        &self.items
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkProjectionItem {
    Document(DocumentId),
    Boundary {
        before: DocumentId,
        after: DocumentId,
    },
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
