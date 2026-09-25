use thiserror::Error;

use crate::DocumentId;

#[derive(Debug, Error)]
pub enum IdError {
    #[error("invalid UUID {value:?}: {source}")]
    InvalidSyntax {
        value: String,
        #[source]
        source: uuid::Error,
    },

    #[error("UUID {0:?} is not canonical lowercase hyphenated text")]
    NonCanonical(String),

    #[error("UUID {0:?} is not version 7")]
    NotVersion7(String),
}

#[derive(Debug, Error)]
pub enum TimestampError {
    #[error("invalid RFC 3339 timestamp {value:?}: {source}")]
    Invalid {
        value: String,
        #[source]
        source: chrono::ParseError,
    },
}

#[derive(Debug, Error)]
pub enum FormatError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("unsupported format version {major}.{minor}")]
    UnsupportedVersion { major: u64, minor: u64 },

    #[error("invalid format name {actual:?}; expected {expected:?}")]
    InvalidFormatName {
        actual: String,
        expected: &'static str,
    },

    #[error("invalid Markdown format {actual:?}; expected {expected:?}")]
    InvalidMarkdownFormat {
        actual: String,
        expected: &'static str,
    },

    #[error("invalid history format {actual:?}; expected {expected:?}")]
    InvalidHistoryFormat {
        actual: String,
        expected: &'static str,
    },

    #[error("work contains document {0} more than once")]
    DuplicateWorkDocument(DocumentId),
}
