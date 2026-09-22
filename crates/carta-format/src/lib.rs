mod error;
mod id;
mod metadata;
mod timestamp;

pub use error::{FormatError, IdError, TimestampError};
pub use id::{ArchiveId, DocumentId, WorkId};
pub use metadata::{
    ArchiveMetadata, DocumentMetadata, FormatVersion, JsonExtensions, WorkMetadata,
};
pub use timestamp::Timestamp;

pub const FORMAT_NAME: &str = "carta-space";
pub const MARKDOWN_FORMAT: &str = "commonmark-0.31.2";
pub const HISTORY_FORMAT: &str = "git";
pub const MIMETYPE: &[u8] = b"application/vnd.carta-space+zip";
