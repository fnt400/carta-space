use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use carta_format::{work_title_key, DocumentId, DocumentMetadata, Timestamp, WorkId, WorkMetadata};
use serde::{Deserialize, Serialize};

use crate::{Archive, DocumentInfo, Error, Volume};

const CONFLICT_DIRECTORY: &str = "carta-conflicts";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictChoice {
    Local,
    External,
}

#[derive(Debug, Clone)]
pub struct DocumentConflict {
    id: String,
    document: DocumentId,
    detected: Timestamp,
    local: Vec<u8>,
    external: Vec<u8>,
    external_missing: bool,
    canonical_directory: PathBuf,
    metadata: Vec<u8>,
}

impl DocumentConflict {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn document(&self) -> DocumentId {
        self.document
    }
    pub fn detected(&self) -> Timestamp {
        self.detected
    }
    pub fn local(&self) -> &[u8] {
        &self.local
    }
    pub fn external(&self) -> &[u8] {
        &self.external
    }
    pub fn external_missing(&self) -> bool {
        self.external_missing
    }
}

#[derive(Debug, Clone)]
pub struct WorkConflict {
    id: String,
    work: WorkId,
    detected: Timestamp,
    local: Vec<u8>,
    external: Vec<u8>,
    external_missing: bool,
    canonical_file: PathBuf,
}

impl WorkConflict {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn work(&self) -> WorkId {
        self.work
    }
    pub fn detected(&self) -> Timestamp {
        self.detected
    }
    pub fn local(&self) -> &[u8] {
        &self.local
    }
    pub fn external(&self) -> &[u8] {
        &self.external
    }
    pub fn external_missing(&self) -> bool {
        self.external_missing
    }
}

#[derive(Debug, Clone)]
pub enum Conflict {
    Document(DocumentConflict),
    Work(WorkConflict),
}

impl Conflict {
    pub fn id(&self) -> &str {
        match self {
            Self::Document(value) => value.id(),
            Self::Work(value) => value.id(),
        }
    }
    pub fn detected(&self) -> Timestamp {
        match self {
            Self::Document(value) => value.detected(),
            Self::Work(value) => value.detected(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum StoredConflict {
    Document {
        id: String,
        document: DocumentId,
        detected: Timestamp,
        local: Vec<u8>,
        external: Vec<u8>,
        #[serde(default)]
        external_missing: bool,
        canonical_directory: PathBuf,
        metadata: Vec<u8>,
    },
    Work {
        id: String,
        work: WorkId,
        detected: Timestamp,
        local: Vec<u8>,
        external: Vec<u8>,
        #[serde(default)]
        external_missing: bool,
        canonical_file: PathBuf,
    },
}

impl Archive {
    pub fn conflicts(&self) -> Result<Vec<Conflict>, Error> {
        let directory = conflict_directory(&self.root);
        if !directory.exists() {
            return Ok(Vec::new());
        }
        let mut paths = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))? {
            let path = entry.map_err(|error| Error::io(&directory, error))?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                paths.push(path);
            }
        }
        paths.sort();
        paths.into_iter().map(|path| read_conflict(&path)).collect()
    }

    pub fn resolve_document_conflict(
        &mut self,
        id: &str,
        choice: ConflictChoice,
        preserve_other: bool,
    ) -> Result<Option<DocumentId>, Error> {
        let path = checked_conflict_path(&self.root, id)?;
        let Conflict::Document(conflict) = read_conflict_or_missing(&path, id)? else {
            return Err(Error::MissingConflict(id.to_owned()));
        };
        if self.documents.contains_key(&conflict.document) {
            self.ensure_document_unlocked(conflict.document)?;
        }
        let directory = self.root.join(&conflict.canonical_directory);
        let canonical = directory.join("content.md");
        let current = fs::read(&canonical);
        let current_matches = match current {
            Ok(current) => !conflict.external_missing && current == conflict.external,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => conflict.external_missing,
            Err(error) => return Err(Error::io(&canonical, error)),
        };
        if !current_matches {
            return Err(Error::StaleConflict(id.to_owned()));
        }
        let (selected, other) = match choice {
            ConflictChoice::Local => (&conflict.local, &conflict.external),
            ConflictChoice::External => (&conflict.external, &conflict.local),
        };
        let selected = std::str::from_utf8(selected).map_err(|error| Error::MalformedConflict {
            path: path.clone(),
            message: format!("selected Document variant is not UTF-8: {error}"),
        })?;
        if selected.contains('\r') {
            return Err(Error::MalformedConflict {
                path: path.clone(),
                message: "selected Document variant has non-canonical line endings".into(),
            });
        }
        let other = if preserve_other {
            Some(
                std::str::from_utf8(other).map_err(|error| Error::MalformedConflict {
                    path: path.clone(),
                    message: format!("other Document variant is not UTF-8: {error}"),
                })?,
            )
        } else {
            None
        };
        let new_document = other.map(|_| (DocumentId::new_v7(), Timestamp::now_local()));
        let mut owned = vec![
            if conflict.external_missing {
                conflict.canonical_directory.clone()
            } else {
                canonical.strip_prefix(&self.root).unwrap().to_path_buf()
            },
            path.strip_prefix(&self.root).unwrap().to_path_buf(),
        ];
        if let Some((new_id, created)) = new_document {
            let volume =
                crate::Volume::from_timestamp(created).ok_or(Error::InvalidVolumeDate(created))?;
            owned.push(
                PathBuf::from("volumes")
                    .join(format!("{:04}", volume.year()))
                    .join(format!("{:02}", volume.month()))
                    .join(new_id.to_string()),
            );
        }
        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before resolving a Document conflict",
            owned,
        )?;
        let operation = (|| {
            if let (Some(other), Some((new_id, created))) = (other, new_document) {
                self.create_document_with(new_id, created, other)?;
            }
            if conflict.external_missing {
                fs::create_dir_all(&directory).map_err(|error| Error::io(&directory, error))?;
                crate::archive::atomic_replace(&directory.join("meta.json"), &conflict.metadata)?;
            }
            crate::archive::atomic_replace(&canonical, selected.as_bytes())?;
            fs::remove_file(&path).map_err(|error| Error::io(&path, error))
        })();
        match operation {
            Ok(()) => {
                transaction.commit()?;
                self.refresh()?;
                Ok(new_document.map(|(id, _)| id))
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub fn resolve_work_conflict(&mut self, id: &str, choice: ConflictChoice) -> Result<(), Error> {
        let path = checked_conflict_path(&self.root, id)?;
        let Conflict::Work(conflict) = read_conflict_or_missing(&path, id)? else {
            return Err(Error::MissingConflict(id.to_owned()));
        };
        if self.works.contains_key(&conflict.work) {
            self.ensure_work_unlocked(conflict.work)?;
        }
        let canonical = self.root.join(&conflict.canonical_file);
        let current = fs::read(&canonical);
        let current_matches = match current {
            Ok(current) => !conflict.external_missing && current == conflict.external,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => conflict.external_missing,
            Err(error) => return Err(Error::io(&canonical, error)),
        };
        if !current_matches {
            return Err(Error::StaleConflict(id.to_owned()));
        }
        let selected = match choice {
            ConflictChoice::Local => &conflict.local,
            ConflictChoice::External => &conflict.external,
        };
        let selected_metadata =
            WorkMetadata::read_from(Cursor::new(selected)).map_err(|error| {
                Error::MalformedConflict {
                    path: path.clone(),
                    message: error.to_string(),
                }
            })?;
        if selected_metadata.id() != conflict.work {
            return Err(Error::MalformedConflict {
                path,
                message: "selected Work identity does not match the conflict".into(),
            });
        }
        let mut unique = HashSet::new();
        for document in selected_metadata.documents() {
            if !unique.insert(*document) {
                return Err(Error::DuplicateWorkDocument(*document));
            }
            if !self.documents.contains_key(document) {
                return Err(Error::DanglingDocumentReference {
                    work: conflict.work,
                    document: *document,
                });
            }
        }
        let title_key = work_title_key(selected_metadata.title());
        if let Some(existing) = self
            .works
            .values()
            .find(|work| work.id() != conflict.work && work_title_key(work.title()) == title_key)
        {
            return Err(Error::WorkTitleConflict {
                title: selected_metadata.title().to_owned(),
                existing: existing.id(),
            });
        }
        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before resolving a Work conflict",
            [
                if conflict.external_missing {
                    canonical
                        .parent()
                        .unwrap()
                        .strip_prefix(&self.root)
                        .unwrap()
                        .to_path_buf()
                } else {
                    canonical.strip_prefix(&self.root).unwrap().to_path_buf()
                },
                path.strip_prefix(&self.root).unwrap().to_path_buf(),
            ],
        )?;
        let operation = (|| {
            if conflict.external_missing {
                let parent = canonical.parent().unwrap();
                fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
            }
            crate::archive::atomic_replace(&canonical, selected)
        })()
        .and_then(|()| fs::remove_file(&path).map_err(|error| Error::io(&path, error)));
        match operation {
            Ok(()) => {
                transaction.commit()?;
                self.refresh()
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub(crate) fn preserve_document_conflict(
        &self,
        document: DocumentId,
        local: &[u8],
        external: &[u8],
        external_missing: bool,
    ) -> Result<String, Error> {
        write_conflict(
            &self.root,
            StoredConflict::Document {
                id: new_conflict_id(),
                document,
                detected: Timestamp::now_local(),
                local: local.to_vec(),
                external: external.to_vec(),
                external_missing,
                canonical_directory: self.documents[&document]
                    .path
                    .strip_prefix(&self.root)
                    .unwrap()
                    .to_path_buf(),
                metadata: self.documents[&document].metadata_bytes.clone(),
            },
        )
    }

    pub(crate) fn preserve_work_conflict(
        &self,
        work: WorkId,
        local: &[u8],
        external: &[u8],
        external_missing: bool,
    ) -> Result<String, Error> {
        write_conflict(
            &self.root,
            StoredConflict::Work {
                id: new_conflict_id(),
                work,
                detected: Timestamp::now_local(),
                local: local.to_vec(),
                external: external.to_vec(),
                external_missing,
                canonical_file: self.works[&work]
                    .path
                    .join("work.json")
                    .strip_prefix(&self.root)
                    .unwrap()
                    .to_path_buf(),
            },
        )
    }
}

pub(crate) fn scrub_document_conflicts(root: &Path, document: DocumentId) -> Result<usize, Error> {
    let directory = conflict_directory(root);
    if !directory.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    for entry in fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))? {
        let path = entry.map_err(|error| Error::io(&directory, error))?.path();
        let remove = match read_conflict(&path) {
            Ok(Conflict::Document(conflict)) => conflict.document == document,
            Ok(Conflict::Work(conflict)) => work_conflict_mentions(&conflict, document),
            Err(_) => false,
        };
        if remove {
            fs::remove_file(&path).map_err(|error| Error::io(&path, error))?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub(crate) fn preserve_recovered_document_conflict(
    root: &Path,
    document: DocumentId,
    current: &[u8],
    previous: &[u8],
    canonical_directory: PathBuf,
    metadata: Vec<u8>,
    external_missing: bool,
) -> Result<(), Error> {
    if conflicts_contain_document(root, document)? {
        return Ok(());
    }
    write_conflict(
        root,
        StoredConflict::Document {
            id: new_conflict_id(),
            document,
            detected: Timestamp::now_local(),
            local: current.to_vec(),
            external: previous.to_vec(),
            external_missing,
            canonical_directory,
            metadata,
        },
    )?;
    Ok(())
}

pub(crate) fn preserve_recovered_work_conflict(
    root: &Path,
    work: WorkId,
    current: &[u8],
    previous: &[u8],
    canonical_file: PathBuf,
    external_missing: bool,
) -> Result<(), Error> {
    if conflicts_contain_work(root, work)? {
        return Ok(());
    }
    write_conflict(
        root,
        StoredConflict::Work {
            id: new_conflict_id(),
            work,
            detected: Timestamp::now_local(),
            local: current.to_vec(),
            external: previous.to_vec(),
            external_missing,
            canonical_file,
        },
    )?;
    Ok(())
}

fn conflicts_contain_document(root: &Path, document: DocumentId) -> Result<bool, Error> {
    let directory = conflict_directory(root);
    if !directory.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))? {
        let path = entry.map_err(|error| Error::io(&directory, error))?.path();
        if matches!(read_conflict(&path), Ok(Conflict::Document(value)) if value.document == document)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn conflicts_contain_work(root: &Path, work: WorkId) -> Result<bool, Error> {
    let directory = conflict_directory(root);
    if !directory.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))? {
        let path = entry.map_err(|error| Error::io(&directory, error))?.path();
        if matches!(read_conflict(&path), Ok(Conflict::Work(value)) if value.work == work) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn missing_document_infos(
    root: &Path,
) -> Result<std::collections::BTreeMap<DocumentId, DocumentInfo>, Error> {
    let directory = conflict_directory(root);
    let mut documents = std::collections::BTreeMap::new();
    if !directory.exists() {
        return Ok(documents);
    }
    for entry in fs::read_dir(&directory).map_err(|error| Error::io(&directory, error))? {
        let path = entry.map_err(|error| Error::io(&directory, error))?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Conflict::Document(conflict) = read_conflict(&path)? else {
            continue;
        };
        if !conflict.external_missing {
            continue;
        }
        let metadata =
            DocumentMetadata::read_from(Cursor::new(&conflict.metadata)).map_err(|error| {
                Error::MalformedConflict {
                    path: path.clone(),
                    message: error.to_string(),
                }
            })?;
        if metadata.id() != conflict.document {
            return Err(Error::MalformedConflict {
                path,
                message: "stored Document metadata identity does not match the conflict".into(),
            });
        }
        let volume = Volume::from_timestamp(metadata.created())
            .ok_or(Error::InvalidVolumeDate(metadata.created()))?;
        documents.insert(
            conflict.document,
            DocumentInfo {
                metadata,
                volume,
                path: root.join(&conflict.canonical_directory),
                metadata_bytes: conflict.metadata,
                content_bytes: conflict.external,
            },
        );
    }
    Ok(documents)
}

fn work_conflict_mentions(conflict: &WorkConflict, document: DocumentId) -> bool {
    [&conflict.local, &conflict.external]
        .into_iter()
        .any(|bytes| {
            WorkMetadata::read_from(Cursor::new(bytes))
                .is_ok_and(|metadata| metadata.documents().contains(&document))
        })
}
fn new_conflict_id() -> String {
    DocumentId::new_v7().to_string()
}
fn conflict_directory(root: &Path) -> PathBuf {
    root.join(".git").join(CONFLICT_DIRECTORY)
}
fn conflict_path(root: &Path, id: &str) -> PathBuf {
    conflict_directory(root).join(format!("{id}.json"))
}
fn checked_conflict_path(root: &Path, id: &str) -> Result<PathBuf, Error> {
    if id
        .parse::<DocumentId>()
        .is_ok_and(|parsed| parsed.to_string() == id)
    {
        Ok(conflict_path(root, id))
    } else {
        Err(Error::MissingConflict(id.to_owned()))
    }
}
fn write_conflict(root: &Path, conflict: StoredConflict) -> Result<String, Error> {
    let id = match &conflict {
        StoredConflict::Document { id, .. } | StoredConflict::Work { id, .. } => id.clone(),
    };
    let directory = conflict_directory(root);
    fs::create_dir_all(&directory).map_err(|error| Error::io(&directory, error))?;
    let path = conflict_path(root, &id);
    let temporary = directory.join(format!(".{id}.tmp"));
    let bytes = serde_json::to_vec_pretty(&conflict).map_err(|error| Error::MalformedConflict {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let result = (|| {
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| Error::io(&temporary, error))?;
        file.write_all(&bytes)
            .map_err(|error| Error::io(&temporary, error))?;
        file.sync_all()
            .map_err(|error| Error::io(&temporary, error))?;
        fs::rename(&temporary, &path).map_err(|error| Error::io(&path, error))?;
        File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| Error::io(&directory, error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result?;
    Ok(id)
}
fn read_conflict_or_missing(path: &Path, id: &str) -> Result<Conflict, Error> {
    if !path.exists() {
        return Err(Error::MissingConflict(id.to_owned()));
    }
    read_conflict(path)
}
fn read_conflict(path: &Path) -> Result<Conflict, Error> {
    let bytes = fs::read(path).map_err(|error| Error::io(path, error))?;
    let stored: StoredConflict =
        serde_json::from_slice(&bytes).map_err(|error| Error::MalformedConflict {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    Ok(match stored {
        StoredConflict::Document {
            id,
            document,
            detected,
            local,
            external,
            external_missing,
            canonical_directory,
            metadata,
        } => Conflict::Document(DocumentConflict {
            id,
            document,
            detected,
            local,
            external,
            external_missing,
            canonical_directory,
            metadata,
        }),
        StoredConflict::Work {
            id,
            work,
            detected,
            local,
            external,
            external_missing,
            canonical_file,
        } => Conflict::Work(WorkConflict {
            id,
            work,
            detected,
            local,
            external,
            external_missing,
            canonical_file,
        }),
    })
}
