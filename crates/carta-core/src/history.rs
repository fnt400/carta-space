use std::fmt;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::str::FromStr;

use carta_format::{work_title_key, DocumentId, DocumentMetadata, Timestamp, WorkId, WorkMetadata};

use crate::{Archive, Document, Error};

const CHECKPOINT_SUBJECT: &str = "Carta checkpoint";
const CHECKPOINT_TRAILER: &str = "Carta-Checkpoint-Kind: ";
const CARTA_AUTHOR_NAME: &str = "Carta Space";
const CARTA_AUTHOR_EMAIL: &str = "history@carta.space";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointKind {
    Automatic,
    Manual,
    Structural,
    Quit,
}

impl CheckpointKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
            Self::Structural => "structural",
            Self::Quit => "quit",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "automatic" => Some(Self::Automatic),
            "manual" => Some(Self::Manual),
            "structural" => Some(Self::Structural),
            "quit" => Some(Self::Quit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckpointId(String);

impl CheckpointId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CheckpointId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for CheckpointId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if (value.len() == 40 || value.len() == 64)
            && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !value.bytes().any(|byte| byte.is_ascii_uppercase())
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(Error::InvalidCheckpointId(value.to_owned()))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    id: CheckpointId,
    created: Timestamp,
    kind: Option<CheckpointKind>,
    note: Option<String>,
}

impl Checkpoint {
    pub fn id(&self) -> &CheckpointId {
        &self.id
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn kind(&self) -> Option<CheckpointKind> {
        self.kind
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }
}

#[derive(Debug, Clone)]
pub struct DocumentRevision {
    checkpoint: Checkpoint,
    document: Document,
}

impl DocumentRevision {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    pub fn document(&self) -> &Document {
        &self.document
    }
}

#[derive(Debug, Clone)]
pub struct WorkSnapshot {
    checkpoint: Checkpoint,
    metadata: WorkMetadata,
    documents: Vec<Document>,
}

impl WorkSnapshot {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    pub fn id(&self) -> WorkId {
        self.metadata.id()
    }

    pub fn created(&self) -> Timestamp {
        self.metadata.created()
    }

    pub fn title(&self) -> &str {
        self.metadata.title()
    }

    pub fn document_ids(&self) -> &[DocumentId] {
        self.metadata.documents()
    }

    pub fn documents(&self) -> &[Document] {
        &self.documents
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkRestoreOptions {
    pub restore_required_trashed_documents: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRestoreImpact {
    affected_other_works: Vec<crate::WorkMembership>,
}

impl HistoryRestoreImpact {
    pub fn affected_other_works(&self) -> &[crate::WorkMembership] {
        &self.affected_other_works
    }
}

impl Archive {
    pub fn is_dirty(&self) -> Result<bool, Error> {
        is_dirty_at(&self.root)
    }

    pub fn checkpoint(
        &self,
        kind: CheckpointKind,
        note: Option<&str>,
    ) -> Result<Option<Checkpoint>, Error> {
        create_checkpoint(&self.root, kind, note)
    }

    pub fn history(&self) -> Result<Vec<Checkpoint>, Error> {
        let output = git_output(
            &self.root,
            "list history",
            &["log", "-z", "--format=%H%x00%cI%x00%B"],
        )?;
        parse_checkpoints(&output.stdout)
    }

    pub fn document_revisions(&self, id: DocumentId) -> Result<Vec<DocumentRevision>, Error> {
        let info = self.documents.get(&id).ok_or(Error::MissingDocument(id))?;
        let relative = relative_path(&self.root, &info.path.join("content.md"))?;
        let output = git_output(
            &self.root,
            "list document history",
            &["log", "-z", "--format=%H%x00%cI%x00%B", "--", &relative],
        )?;
        let checkpoints = parse_checkpoints(&output.stdout)?;
        checkpoints
            .into_iter()
            .map(|checkpoint| {
                let document = read_document_at(&self.root, id, checkpoint.id())?;
                Ok(DocumentRevision {
                    checkpoint,
                    document,
                })
            })
            .collect()
    }

    pub fn read_document_revision(
        &self,
        id: DocumentId,
        checkpoint: &CheckpointId,
    ) -> Result<Document, Error> {
        read_document_at(&self.root, id, checkpoint)
    }

    pub fn restore_document_version(
        &mut self,
        id: DocumentId,
        checkpoint: &CheckpointId,
    ) -> Result<Checkpoint, Error> {
        self.ensure_document_unlocked(id)?;
        self.require_clean_restore()?;
        let historical = read_document_at(&self.root, id, checkpoint)?;
        let info = self.documents.get(&id).ok_or(Error::MissingDocument(id))?;
        self.ensure_document_current(info)?;
        let path = info.path.join("content.md");
        let metadata_path = info.path.join("meta.json");
        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before Document History restore",
            [
                relative_path(&self.root, &path)?.into(),
                relative_path(&self.root, &metadata_path)?.into(),
            ],
        )?;
        let operation = self.edit_document(id, historical.content()).and_then(|()| {
            require_checkpoint(
                &self.root,
                CheckpointKind::Structural,
                Some("Restored a Document version"),
            )
        });
        match operation {
            Ok(created) => {
                transaction.commit()?;
                self.refresh()?;
                Ok(created)
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub fn document_restore_impact(&self, id: DocumentId) -> Result<HistoryRestoreImpact, Error> {
        if !self.documents.contains_key(&id) {
            return Err(Error::MissingDocument(id));
        }
        Ok(HistoryRestoreImpact {
            affected_other_works: self
                .works
                .values()
                .filter(|work| work.documents().contains(&id))
                .map(|work| crate::WorkMembership {
                    id: work.id(),
                    title: work.title().to_owned(),
                })
                .collect(),
        })
    }

    pub fn restore_document_version_as_new(
        &mut self,
        id: DocumentId,
        checkpoint: &CheckpointId,
    ) -> Result<(DocumentId, Checkpoint), Error> {
        self.require_clean_restore()?;
        let historical = read_document_at(&self.root, id, checkpoint)?;
        let new_id = self.create_document(historical.content())?;
        let created_path = self
            .documents
            .get(&new_id)
            .expect("new document was inserted")
            .path
            .clone();
        match require_checkpoint(
            &self.root,
            CheckpointKind::Structural,
            Some("Restored a Document version as a new Document"),
        ) {
            Ok(created) => Ok((new_id, created)),
            Err(error) => {
                rollback_files(&self.root, &[], &[created_path], error)?;
                unreachable!("rollback_files returns the original error")
            }
        }
    }

    pub fn work_snapshot(
        &self,
        id: WorkId,
        checkpoint: &CheckpointId,
    ) -> Result<WorkSnapshot, Error> {
        read_work_snapshot(&self.root, id, checkpoint)
    }

    pub fn restore_work_version(
        &mut self,
        id: WorkId,
        checkpoint: &CheckpointId,
        options: WorkRestoreOptions,
    ) -> Result<Checkpoint, Error> {
        self.ensure_work_unlocked(id)?;
        self.require_clean_restore()?;
        let snapshot = read_work_snapshot(&self.root, id, checkpoint)?;
        for document in &snapshot.documents {
            let document_id = document.metadata().id();
            if self.documents.contains_key(&document_id) {
                self.ensure_document_unlocked(document_id)?;
            }
        }
        let current_work = self.works.get(&id).ok_or(Error::MissingWork(id))?;
        self.ensure_work_current(current_work)?;

        let title_key = work_title_key(snapshot.title());
        if let Some(existing) = self
            .works
            .values()
            .find(|work| work.id() != id && work_title_key(work.title()) == title_key)
        {
            return Err(Error::WorkTitleConflict {
                title: snapshot.title().to_owned(),
                existing: existing.id(),
            });
        }

        let missing: Vec<_> = snapshot
            .metadata
            .documents()
            .iter()
            .copied()
            .filter(|document| !self.documents.contains_key(document))
            .collect();
        if !missing.is_empty() && !options.restore_required_trashed_documents {
            return Err(Error::TrashedDocumentConsentRequired {
                work: id,
                documents: missing,
            });
        }

        let work_path = current_work.path.join("work.json");
        let restored_metadata = current_work
            .metadata
            .with_title(snapshot.title().to_owned())
            .with_documents(snapshot.document_ids().to_vec())
            .map_err(|error| Error::format(&work_path, error))?;
        let mut cleanup = vec![relative_path(&self.root, &work_path)?.into()];
        for document in &snapshot.documents {
            if let Some(current) = self.documents.get(&document.metadata().id()) {
                cleanup.push(relative_path(&self.root, &current.path.join("content.md"))?.into());
                cleanup.push(relative_path(&self.root, &current.path.join("meta.json"))?.into());
            } else {
                let volume = crate::Volume::from_timestamp(document.metadata().created())
                    .ok_or(Error::InvalidVolumeDate(document.metadata().created()))?;
                cleanup.push(
                    PathBuf::from("volumes")
                        .join(format!("{:04}", volume.year()))
                        .join(format!("{:02}", volume.month()))
                        .join(document.metadata().id().to_string()),
                );
            }
        }
        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before Work History restore",
            cleanup,
        )?;

        let restore = (|| {
            for document in &snapshot.documents {
                let document_id = document.metadata().id();
                if self.documents.contains_key(&document_id) {
                    self.edit_document(document_id, document.content())?;
                } else {
                    let volume = crate::Volume::from_timestamp(document.metadata().created())
                        .ok_or(Error::InvalidVolumeDate(document.metadata().created()))?;
                    let destination = self
                        .root
                        .join("volumes")
                        .join(format!("{:04}", volume.year()))
                        .join(format!("{:02}", volume.month()))
                        .join(document_id.to_string());
                    write_restored_document(&destination, document)?;
                }
            }

            let work_bytes = serialize_work_metadata(&work_path, &restored_metadata)?;
            atomic_replace(&work_path, &work_bytes)?;
            require_checkpoint(
                &self.root,
                CheckpointKind::Structural,
                Some("Restored a Work version"),
            )
        })();
        match restore {
            Ok(created) => {
                transaction.commit()?;
                self.refresh()?;
                Ok(created)
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub fn work_restore_impact(
        &self,
        id: WorkId,
        checkpoint: &CheckpointId,
    ) -> Result<HistoryRestoreImpact, Error> {
        let snapshot = read_work_snapshot(&self.root, id, checkpoint)?;
        let mut affected = Vec::new();
        for work in self.works.values().filter(|work| work.id() != id) {
            let changes_shared_content = snapshot.documents.iter().any(|historical| {
                work.documents().contains(&historical.metadata().id())
                    && self
                        .read_document(historical.metadata().id())
                        .is_ok_and(|current| current.content() != historical.content())
            });
            if changes_shared_content {
                affected.push(crate::WorkMembership {
                    id: work.id(),
                    title: work.title().to_owned(),
                });
            }
        }
        Ok(HistoryRestoreImpact {
            affected_other_works: affected,
        })
    }

    fn require_clean_restore(&self) -> Result<(), Error> {
        if self.is_dirty()? {
            Err(Error::RestoreRequiresCleanArchive)
        } else {
            Ok(())
        }
    }
}

pub(crate) fn create_initial_checkpoint(root: &Path) -> Result<(), Error> {
    require_checkpoint(root, CheckpointKind::Structural, Some("Created Archive"))?;
    Ok(())
}

pub(crate) fn create_package_checkpoint(root: &Path) -> Result<Option<Checkpoint>, Error> {
    create_checkpoint_with_stage_args(
        root,
        CheckpointKind::Automatic,
        Some("Prepared portable package"),
        &[
            "add",
            "--all",
            "--",
            ".",
            ":(exclude)**/.carta-*",
            ":(exclude).carta-*",
        ],
    )
}

pub(crate) fn is_dirty_at(root: &Path) -> Result<bool, Error> {
    let output = git_output(
        root,
        "inspect current changes",
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    Ok(!output.stdout.is_empty())
}

fn create_checkpoint(
    root: &Path,
    kind: CheckpointKind,
    note: Option<&str>,
) -> Result<Option<Checkpoint>, Error> {
    create_checkpoint_with_stage_args(root, kind, note, &["add", "--all", "--", "."])
}

fn create_checkpoint_with_stage_args(
    root: &Path,
    kind: CheckpointKind,
    note: Option<&str>,
    stage_args: &[&str],
) -> Result<Option<Checkpoint>, Error> {
    if !is_dirty_at(root)? {
        return Ok(None);
    }
    let message = checkpoint_message(kind, note)?;
    if let Err(error) = git_output(root, "prepare checkpoint", stage_args) {
        let _ = git_output(
            root,
            "clear checkpoint preparation",
            &["reset", "--mixed", "--quiet"],
        );
        return Err(error);
    }
    if git_output(
        root,
        "inspect prepared checkpoint",
        &["diff", "--cached", "--name-only", "-z"],
    )?
    .stdout
    .is_empty()
    {
        return Ok(None);
    }
    let result = git_with_input(
        root,
        "create checkpoint",
        &[
            "-c",
            &format!("user.name={CARTA_AUTHOR_NAME}"),
            "-c",
            &format!("user.email={CARTA_AUTHOR_EMAIL}"),
            "commit",
            "--quiet",
            "--file=-",
        ],
        message.as_bytes(),
    );
    if let Err(error) = result {
        let _ = git_output(
            root,
            "clear checkpoint preparation",
            &["reset", "--mixed", "--quiet"],
        );
        return Err(error);
    }
    Ok(Some(current_checkpoint(root)?))
}

pub(crate) fn require_checkpoint(
    root: &Path,
    kind: CheckpointKind,
    note: Option<&str>,
) -> Result<Checkpoint, Error> {
    create_checkpoint(root, kind, note)?.ok_or(Error::NoChangesToCheckpoint)
}

fn checkpoint_message(kind: CheckpointKind, note: Option<&str>) -> Result<String, Error> {
    if note.is_some_and(|value| value.contains('\0')) {
        return Err(Error::InvalidCheckpointNote);
    }
    let mut message = format!("{CHECKPOINT_SUBJECT}: {}\n", kind.as_str());
    if let Some(note) = note.map(str::trim).filter(|note| !note.is_empty()) {
        message.push('\n');
        message.push_str(note);
        message.push('\n');
    }
    message.push('\n');
    message.push_str(CHECKPOINT_TRAILER);
    message.push_str(kind.as_str());
    message.push('\n');
    Ok(message)
}

fn current_checkpoint(root: &Path) -> Result<Checkpoint, Error> {
    let output = git_output(
        root,
        "read created checkpoint",
        &["show", "-s", "-z", "--format=%H%x00%cI%x00%B", "HEAD"],
    )?;
    let mut checkpoints = parse_checkpoints(&output.stdout)?;
    checkpoints
        .pop()
        .ok_or_else(|| Error::MalformedHistory("created checkpoint is missing".to_owned()))
}

fn parse_checkpoints(bytes: &[u8]) -> Result<Vec<Checkpoint>, Error> {
    let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
    let fields = fields.strip_suffix(&[&[][..]]).unwrap_or(&fields);
    if fields.len() % 3 != 0 {
        return Err(Error::MalformedHistory(
            "checkpoint listing has incomplete records".to_owned(),
        ));
    }
    fields
        .chunks_exact(3)
        .map(|fields| {
            let id = text_field(fields[0], "identifier")?.parse()?;
            let created = text_field(fields[1], "timestamp")?
                .parse()
                .map_err(|error| Error::MalformedHistory(format!("invalid timestamp: {error}")))?;
            let message = text_field(fields[2], "message")?;
            let (kind, note) = parse_checkpoint_message(message);
            Ok(Checkpoint {
                id,
                created,
                kind,
                note,
            })
        })
        .collect()
}

fn parse_checkpoint_message(message: &str) -> (Option<CheckpointKind>, Option<String>) {
    let message = message.trim_end_matches('\n');
    let mut lines: Vec<_> = message.lines().collect();
    let kind = lines
        .last()
        .and_then(|line| line.strip_prefix(CHECKPOINT_TRAILER))
        .and_then(CheckpointKind::parse);
    if kind.is_some() {
        lines.pop();
        while lines.last() == Some(&"") {
            lines.pop();
        }
        if lines
            .first()
            .is_some_and(|line| line.starts_with(CHECKPOINT_SUBJECT))
        {
            lines.remove(0);
        }
        while lines.first() == Some(&"") {
            lines.remove(0);
        }
    }
    let note = (!lines.is_empty()).then(|| lines.join("\n"));
    (kind, note)
}

fn text_field<'a>(bytes: &'a [u8], name: &str) -> Result<&'a str, Error> {
    std::str::from_utf8(bytes)
        .map_err(|error| Error::MalformedHistory(format!("{name} is not UTF-8: {error}")))
}

pub(crate) fn read_document_at(
    root: &Path,
    id: DocumentId,
    checkpoint: &CheckpointId,
) -> Result<Document, Error> {
    let directory = historical_document_directory(root, id, checkpoint)?.ok_or(
        Error::MissingDocumentRevision {
            document: id,
            checkpoint: checkpoint.to_string(),
        },
    )?;
    let metadata_path = format!("{directory}/meta.json");
    let content_path = format!("{directory}/content.md");
    let metadata_bytes = read_blob(root, checkpoint, &metadata_path).map_err(|_| {
        Error::MissingDocumentRevision {
            document: id,
            checkpoint: checkpoint.to_string(),
        }
    })?;
    let metadata = DocumentMetadata::read_from(Cursor::new(metadata_bytes))
        .map_err(|error| Error::format(metadata_path, error))?;
    if metadata.id() != id {
        return Err(Error::MissingDocumentRevision {
            document: id,
            checkpoint: checkpoint.to_string(),
        });
    }
    let content_bytes =
        read_blob(root, checkpoint, &content_path).map_err(|_| Error::MissingDocumentRevision {
            document: id,
            checkpoint: checkpoint.to_string(),
        })?;
    let content = String::from_utf8(content_bytes).map_err(|error| {
        Error::MalformedHistory(format!(
            "Document {id} content at {checkpoint} is not UTF-8: {error}"
        ))
    })?;
    if content.contains('\r') {
        return Err(Error::MalformedHistory(format!(
            "Document {id} content at {checkpoint} has non-canonical line endings"
        )));
    }
    Ok(Document::new(metadata, content))
}

fn historical_document_directory(
    root: &Path,
    id: DocumentId,
    checkpoint: &CheckpointId,
) -> Result<Option<String>, Error> {
    let output = git_output(
        root,
        "locate historical Document",
        &[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            checkpoint.as_str(),
            "--",
            "volumes",
        ],
    )?;
    let suffix = format!("/{id}/meta.json");
    let mut found = None;
    for path in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = text_field(path, "historical path")?;
        if let Some(directory) = path.strip_suffix(&suffix) {
            let directory = format!("{directory}/{id}");
            if found.replace(directory).is_some() {
                return Err(Error::MalformedHistory(format!(
                    "Document {id} occurs more than once at {checkpoint}"
                )));
            }
        }
    }
    Ok(found)
}

pub(crate) fn read_work_snapshot(
    root: &Path,
    id: WorkId,
    checkpoint: &CheckpointId,
) -> Result<WorkSnapshot, Error> {
    let path = format!("works/{id}/work.json");
    let bytes = read_blob(root, checkpoint, &path).map_err(|_| Error::MissingWorkRevision {
        work: id,
        checkpoint: checkpoint.to_string(),
    })?;
    let metadata =
        WorkMetadata::read_from(Cursor::new(bytes)).map_err(|error| Error::format(&path, error))?;
    if metadata.id() != id {
        return Err(Error::MissingWorkRevision {
            work: id,
            checkpoint: checkpoint.to_string(),
        });
    }
    let checkpoint_data = checkpoint_by_id(root, checkpoint)?;
    let mut documents = Vec::with_capacity(metadata.documents().len());
    for document in metadata.documents() {
        let historical = read_document_at(root, *document, checkpoint).map_err(|_| {
            Error::UnrecoverableWorkDocument {
                work: id,
                document: *document,
                checkpoint: checkpoint.to_string(),
            }
        })?;
        documents.push(historical);
    }
    Ok(WorkSnapshot {
        checkpoint: checkpoint_data,
        metadata,
        documents,
    })
}

fn checkpoint_by_id(root: &Path, checkpoint: &CheckpointId) -> Result<Checkpoint, Error> {
    let output = git_output(
        root,
        "read checkpoint",
        &[
            "show",
            "-s",
            "-z",
            "--format=%H%x00%cI%x00%B",
            checkpoint.as_str(),
        ],
    )?;
    let mut checkpoints = parse_checkpoints(&output.stdout)?;
    checkpoints
        .pop()
        .ok_or_else(|| Error::MalformedHistory("checkpoint is missing".to_owned()))
}

pub(crate) fn read_blob(
    root: &Path,
    checkpoint: &CheckpointId,
    path: &str,
) -> Result<Vec<u8>, Error> {
    let object = format!("{}:{path}", checkpoint.as_str());
    Ok(git_output(
        root,
        "read historical content",
        &["cat-file", "blob", &object],
    )?
    .stdout)
}

pub(crate) fn write_restored_document(
    destination: &Path,
    document: &Document,
) -> Result<(), Error> {
    let parent = destination
        .parent()
        .expect("Document destinations always have a parent");
    fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    let staging = parent.join(format!(".carta-restore-{}", document.metadata().id()));
    fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
    let result = (|| {
        let metadata_path = staging.join("meta.json");
        let mut metadata = Vec::new();
        document
            .metadata()
            .write_to(&mut metadata)
            .map_err(|error| Error::format(&metadata_path, error))?;
        fs::write(&metadata_path, metadata).map_err(|error| Error::io(&metadata_path, error))?;
        let content_path = staging.join("content.md");
        fs::write(&content_path, document.content())
            .map_err(|error| Error::io(&content_path, error))?;
        fs::rename(&staging, destination).map_err(|error| Error::io(destination, error))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn rollback_files(
    root: &Path,
    originals: &[(PathBuf, Vec<u8>)],
    created_directories: &[PathBuf],
    original_error: Error,
) -> Result<(), Error> {
    let rollback = (|| {
        for (path, bytes) in originals.iter().rev() {
            atomic_replace(path, bytes)?;
        }
        for path in created_directories.iter().rev() {
            fs::remove_dir_all(path).map_err(|error| Error::io(path, error))?;
        }
        git_output(
            root,
            "clear failed restore",
            &["reset", "--mixed", "--quiet"],
        )?;
        Ok(())
    })();
    match rollback {
        Ok(()) => Err(original_error),
        Err(rollback) => Err(Error::RestoreRollbackFailed {
            operation: Box::new(original_error),
            rollback: Box::new(rollback),
        }),
    }
}

fn relative_path(root: &Path, path: &Path) -> Result<String, Error> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| Error::PathOutsideArchive(path.to_path_buf()))?;
    relative
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::NonUtf8CanonicalPath(relative.to_path_buf()))
}

pub(crate) fn git_output(
    root: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<Output, Error> {
    let output = git_command(root)
        .args(args)
        .output()
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(Error::GitCommandFailed {
            operation,
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

pub(crate) fn git_with_input(
    root: &Path,
    operation: &'static str,
    args: &[&str],
    input: &[u8],
) -> Result<Output, Error> {
    let mut child = git_command(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })?;
    child
        .stdin
        .take()
        .expect("piped Git stdin is available")
        .write_all(input)
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })?;
    let output = child
        .wait_with_output()
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(Error::GitCommandFailed {
            operation,
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

pub(crate) fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(["--git-dir=.git", "--work-tree=."]);
    command
}

fn serialize_work_metadata(path: &Path, metadata: &WorkMetadata) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    metadata
        .write_to(&mut bytes)
        .map_err(|error| Error::format(path, error))?;
    Ok(bytes)
}

fn atomic_replace(path: &Path, content: &[u8]) -> Result<(), Error> {
    crate::archive::atomic_replace(path, content)
}
