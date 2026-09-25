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

    #[error("export destination must be outside the Archive: {0}")]
    DestinationInsideArchive(PathBuf),

    #[error("export destination is not a replaceable regular file: {0}")]
    UnsafeDestination(PathBuf),

    #[error("export destination parent does not exist or is not a directory: {0}")]
    InvalidDestinationParent(PathBuf),

    #[error("archive resource path is not UTF-8 and cannot be packaged: {0}")]
    NonUtf8PackagePath(PathBuf),

    #[error("external temporary path is not UTF-8: {0}")]
    NonUtf8TemporaryPath(PathBuf),

    #[error("external temporary registry is malformed: {0}")]
    MalformedTemporaryRegistry(String),

    #[error("symbolic links cannot be represented safely in a portable package: {0}")]
    UnsupportedPackageSymlink(PathBuf),

    #[error("ZIP error at {path}: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("portable package validation failed: {0}")]
    InvalidPackage(String),

    #[error("Pandoc is unavailable at {program}: {source}")]
    PandocUnavailable {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Pandoc PDF export failed with status {status}: {stderr}")]
    PdfExportFailed {
        status: std::process::ExitStatus,
        stderr: String,
    },

    #[error("Pandoc reported success but did not produce a regular PDF file")]
    PdfOutputMissing,

    #[error("timestamp cannot be represented as a YYYY/MM volume: {0}")]
    InvalidVolumeDate(carta_format::Timestamp),

    #[error("document does not exist: {0}")]
    MissingDocument(DocumentId),

    #[error("work does not exist: {0}")]
    MissingWork(WorkId),

    #[error("canonical file changed outside this loaded Archive: {0}")]
    ExternalChange(PathBuf),

    #[error("external divergence was preserved as conflict {0}")]
    ConflictPreserved(String),

    #[error("conflict record is missing: {0}")]
    MissingConflict(String),

    #[error("conflict record is malformed at {path}: {message}")]
    MalformedConflict { path: PathBuf, message: String },

    #[error("conflict {0} is stale because the external variant changed again")]
    StaleConflict(String),

    #[error("byte offset {offset} is not a UTF-8 character boundary in document {document}")]
    InvalidByteBoundary { document: DocumentId, offset: usize },

    #[error("import content is not valid UTF-8: {0}")]
    InvalidImportUtf8(#[from] std::str::Utf8Error),

    #[error("whole-archive search queries must be single-line")]
    MultilineSearchQuery,

    #[error("document {document} is not a member of work {work}")]
    DocumentNotInWork { work: WorkId, document: DocumentId },

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

    #[error("Git failed while attempting to {operation} with status {status}: {stderr}")]
    GitCommandFailed {
        operation: &'static str,
        status: std::process::ExitStatus,
        stderr: String,
    },

    #[error("invalid checkpoint identifier: {0}")]
    InvalidCheckpointId(String),

    #[error("checkpoint note contains a NUL character")]
    InvalidCheckpointNote,

    #[error("history data is malformed: {0}")]
    MalformedHistory(String),

    #[error("there are no current changes to checkpoint")]
    NoChangesToCheckpoint,

    #[error("restore requires an Archive with no uncheckpointed changes")]
    RestoreRequiresCleanArchive,

    #[error("Document {document} has no recoverable version at checkpoint {checkpoint}")]
    MissingDocumentRevision {
        document: DocumentId,
        checkpoint: String,
    },

    #[error("Work {work} has no recoverable version at checkpoint {checkpoint}")]
    MissingWorkRevision { work: WorkId, checkpoint: String },

    #[error("Work {work} cannot be restored because Document {document} is unrecoverable at checkpoint {checkpoint}")]
    UnrecoverableWorkDocument {
        work: WorkId,
        document: DocumentId,
        checkpoint: String,
    },

    #[error("restoring Work {work} requires explicit consent to restore trashed Documents {documents:?}")]
    TrashedDocumentConsentRequired {
        work: WorkId,
        documents: Vec<DocumentId>,
    },

    #[error("canonical path is outside the Archive: {0}")]
    PathOutsideArchive(PathBuf),

    #[error("canonical path is not UTF-8: {0}")]
    NonUtf8CanonicalPath(PathBuf),

    #[error("restore failed ({operation}) and rollback also failed ({rollback})")]
    RestoreRollbackFailed {
        operation: Box<Error>,
        rollback: Box<Error>,
    },

    #[error("structural operation requires no tracked working-tree changes")]
    StructuralOperationRequiresCleanArchive,

    #[error("document is active and cannot be restored or wiped: {0}")]
    DocumentIsActive(DocumentId),

    #[error("work is active and cannot be restored: {0}")]
    WorkIsActive(WorkId),

    #[error("document has no recoverable trashed state: {0}")]
    DocumentNotRecoverable(DocumentId),

    #[error("work has no recoverable trashed state: {0}")]
    WorkNotRecoverable(WorkId),

    #[error("wipe confirmation did not match the planned token")]
    WipeConfirmationMismatch,

    #[error("wipe plan is stale because retained refs or current state changed")]
    StaleWipePlan,

    #[error("cannot strictly wipe document {document}: content object {object} is also retained at {path}")]
    WipeContentShared {
        document: DocumentId,
        object: String,
        path: String,
    },

    #[error("wipe cannot rewrite retained ref {reference} whose target is a {object_type} object")]
    UnsupportedWipeRef {
        reference: String,
        object_type: String,
    },

    #[error("wipe requires the Archive repository to have only its primary working tree")]
    WipeAdditionalWorktrees,

    #[error("wipe refuses Git pseudoref or operation state: {0}")]
    WipeRepositoryState(String),

    #[error("wipe verification failed: {0}")]
    WipeVerificationFailed(String),

    #[error("wipe failed ({operation}) and restoring original refs also failed ({rollback})")]
    WipeRollbackFailed {
        operation: Box<Error>,
        rollback: Box<Error>,
    },

    #[error("wipe reached irreversible cleanup and then failed during {stage}: {source}")]
    WipeIrreversibleFailure {
        stage: &'static str,
        source: Box<Error>,
    },

    #[error("structural operation failed ({operation}) and rollback also failed ({rollback})")]
    StructuralRollbackFailed {
        operation: Box<Error>,
        rollback: Box<Error>,
    },

    #[error("structural transaction manifest is malformed: {0}")]
    MalformedTransaction(String),

    #[error("interrupted transaction recovery is ambiguous because tracked external changes exist at {0:?}")]
    AmbiguousTransactionRecovery(Vec<PathBuf>),

    #[error("structural transaction contains an unsafe or pre-existing owned path: {0}")]
    UnsafeTransactionPath(PathBuf),
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

    pub(crate) fn zip(path: impl AsRef<Path>, source: zip::result::ZipError) -> Self {
        Self::Zip {
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
