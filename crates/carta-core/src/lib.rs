mod archive;
mod document;
mod error;
mod history;
mod validation;
mod work;

pub use archive::Archive;
pub use document::{Document, DocumentInfo, Volume};
pub use error::{Error, ValidationErrors, ValidationIssue, ValidationIssueKind};
pub use history::{
    Checkpoint, CheckpointId, CheckpointKind, DocumentRevision, WorkRestoreOptions, WorkSnapshot,
};
pub use work::{Work, WorkProjection, WorkProjectionItem};

pub use carta_format::{ArchiveId, DocumentId, Timestamp, WorkId};
