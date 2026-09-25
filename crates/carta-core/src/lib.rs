mod archive;
mod content;
mod document;
mod error;
mod history;
mod retrieval;
mod validation;
mod work;

pub use archive::Archive;
pub use content::{
    extract_markdown_links, link_at_byte_offset, CartaLinkTarget, LinkResolution, MarkdownLink,
};
pub use document::{Document, DocumentInfo, Volume};
pub use error::{Error, ValidationErrors, ValidationIssue, ValidationIssueKind};
pub use history::{
    Checkpoint, CheckpointId, CheckpointKind, DocumentRevision, WorkRestoreOptions, WorkSnapshot,
};
pub use retrieval::{
    Backlink, DocumentTextRegion, LeapDirection, LeapMatch, LeapPosition, LeapRuntime, LeapSession,
    SearchResult,
};
pub use work::{Work, WorkProjection, WorkProjectionItem};

pub use carta_format::{ArchiveId, DocumentId, Timestamp, WorkId};
