mod archive;
mod conflict;
mod content;
mod document;
mod error;
mod history;
mod package;
mod retrieval;
mod sync;
mod transaction;
mod trash;
mod validation;
mod work;

pub use archive::Archive;
pub use conflict::{Conflict, ConflictChoice, DocumentConflict, WorkConflict};
pub use content::{
    extract_markdown_links, link_at_byte_offset, CartaLinkTarget, LinkResolution, MarkdownLink,
};
pub use document::{Document, DocumentInfo, Volume};
pub use error::{Error, ValidationErrors, ValidationIssue, ValidationIssueKind};
pub use history::{
    Checkpoint, CheckpointId, CheckpointKind, DocumentRevision, HistoryRestoreImpact,
    WorkRestoreOptions, WorkSnapshot,
};
pub use package::PackageReport;
pub use retrieval::{
    Backlink, DocumentTextRegion, LeapDirection, LeapMatch, LeapPosition, LeapRuntime, LeapSession,
    SearchResult,
};
pub use sync::{SyncOutcome, SyncReport};
pub use trash::{
    DocumentTrashImpact, TrashInventory, TrashedDocument, TrashedWork, WipePlan, WipeReport,
    WorkMembership,
};
pub use work::{Work, WorkProjection, WorkProjectionItem};

pub use carta_format::{ArchiveId, DocumentId, Timestamp, WorkId};
