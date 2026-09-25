use std::fmt;
use std::path::{Path, PathBuf};

use carta_format::{DocumentId, WorkId};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("format error at {path}: {source}")]
    Format {
        path: PathBuf,
        #[source]
        source: carta_format::FormatError,
    },

    #[error(transparent)]
    InvalidArchive(#[from] ValidationErrors),

    #[error("archive destination already exists: {0}")]
    AlreadyExists(PathBuf),

    #[error("timestamp cannot be represented as a YYYY/MM volume: {0}")]
    InvalidVolumeDate(carta_format::Timestamp),

    #[error("document does not exist: {0}")]
    MissingDocument(DocumentId),

    #[error("work {work} references missing document {document}")]
    DanglingDocumentReference { work: WorkId, document: DocumentId },

    #[error("work contains document {0} more than once")]
    DuplicateWorkDocument(DocumentId),

    #[error("active work title conflicts with work {existing}: {title:?}")]
    WorkTitleConflict { title: String, existing: WorkId },

    #[error("could not start Git while creating {path}: {source}")]
    GitUnavailable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("git init failed for {path} with status {status}")]
    GitInitFailed {
        path: PathBuf,
        status: std::process::ExitStatus,
    },
}

impl Error {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub(crate) fn format(path: impl AsRef<Path>, source: carta_format::FormatError) -> Self {
        Self::Format {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

#[derive(Debug)]
pub struct ValidationErrors {
    issues: Vec<ValidationIssue>,
}

impl ValidationErrors {
    pub(crate) fn new(issues: Vec<ValidationIssue>) -> Self {
        debug_assert!(!issues.is_empty());
        Self { issues }
    }

    pub fn issues(&self) -> &[ValidationIssue] {
        &self.issues
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "archive validation failed with {} issue(s)",
            self.issues.len()
        )
    }
}

impl std::error::Error for ValidationErrors {}

#[derive(Debug)]
pub struct ValidationIssue {
    path: PathBuf,
    kind: ValidationIssueKind,
}

impl ValidationIssue {
    pub(crate) fn new(path: impl Into<PathBuf>, kind: ValidationIssueKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn kind(&self) -> &ValidationIssueKind {
        &self.kind
    }
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path.display(), self.kind)
    }
}

#[derive(Debug, Error)]
pub enum ValidationIssueKind {
    #[error("required entry is missing")]
    MissingEntry,

    #[error("expected {expected}, found {actual}")]
    WrongEntryType {
        expected: &'static str,
        actual: &'static str,
    },

    #[error("mimetype content is not exactly application/vnd.carta-space+zip")]
    InvalidMimetype,

    #[error("archive root is not a Git working tree: {0}")]
    InvalidGitWorkingTree(String),

    #[error("invalid format data: {0}")]
    InvalidFormat(String),

    #[error("content.md is not valid UTF-8")]
    InvalidUtf8,

    #[error("content.md contains non-LF line endings")]
    NonCanonicalLineEndings,

    #[error("malformed entry in reserved namespace: {0}")]
    MalformedReservedEntry(String),

    #[error("directory ID {directory_id:?} does not equal metadata ID {metadata_id:?}")]
    DirectoryIdMismatch {
        directory_id: String,
        metadata_id: String,
    },

    #[error("document timestamp belongs to {expected_year:04}/{expected_month:02}, not {actual_year:04}/{actual_month:02}")]
    VolumeMismatch {
        expected_year: i32,
        expected_month: u32,
        actual_year: u16,
        actual_month: u8,
    },

    #[error("duplicate document ID {0}")]
    DuplicateDocument(DocumentId),

    #[error("duplicate work ID {0}")]
    DuplicateWork(WorkId),

    #[error("duplicate active Work title shared with {other}")]
    DuplicateWorkTitle { other: WorkId },

    #[error("work {work} references missing document {document}")]
    DanglingDocumentReference { work: WorkId, document: DocumentId },

    #[error("I/O error: {0}")]
    Io(String),
}
