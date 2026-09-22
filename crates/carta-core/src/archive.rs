use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use carta_format::{
    ArchiveMetadata, DocumentId, DocumentMetadata, Timestamp, WorkId, WorkMetadata, MIMETYPE,
};

use crate::validation::scan_archive;
use crate::{Document, DocumentInfo, Error, ValidationErrors, Volume, Work};

#[derive(Debug)]
pub struct Archive {
    root: PathBuf,
    metadata: ArchiveMetadata,
    documents: BTreeMap<DocumentId, DocumentInfo>,
    works: BTreeMap<WorkId, Work>,
}

impl Archive {
    pub fn create(path: impl AsRef<Path>) -> Result<Self, Error> {
        let destination = path.as_ref();
        match fs::symlink_metadata(destination) {
            Ok(_) => return Err(Error::AlreadyExists(destination.to_path_buf())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(destination, error)),
        }

        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let archive_id = carta_format::ArchiveId::new_v7();
        let staging = parent.join(format!(".carta-create-{archive_id}"));
        fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
        let mut guard = CleanupDirectory::new(staging.clone());

        fs::write(staging.join("mimetype"), MIMETYPE)
            .map_err(|error| Error::io(staging.join("mimetype"), error))?;

        let metadata = ArchiveMetadata::new(archive_id, Timestamp::now_local());
        write_archive_metadata(&staging.join("carta.json"), &metadata)?;
        fs::create_dir(staging.join("volumes"))
            .map_err(|error| Error::io(staging.join("volumes"), error))?;
        fs::create_dir(staging.join("works"))
            .map_err(|error| Error::io(staging.join("works"), error))?;

        let status = Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(&staging)
            .status()
            .map_err(|source| Error::GitUnavailable {
                path: staging.clone(),
                source,
            })?;
        if !status.success() {
            return Err(Error::GitInitFailed {
                path: staging,
                status,
            });
        }

        scan_archive(&guard.path)?;
        fs::rename(&guard.path, destination).map_err(|error| Error::io(destination, error))?;
        guard.disarm();
        Self::open(destination)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let root = path.as_ref().to_path_buf();
        let scanned = scan_archive(&root)?;
        Ok(Self {
            root,
            metadata: scanned.metadata,
            documents: scanned.documents,
            works: scanned.works,
        })
    }

    pub fn validate(path: impl AsRef<Path>) -> Result<(), ValidationErrors> {
        scan_archive(path.as_ref()).map(|_| ())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn metadata(&self) -> &ArchiveMetadata {
        &self.metadata
    }

    pub fn documents(&self) -> impl Iterator<Item = &DocumentInfo> {
        self.documents.values()
    }

    pub fn works(&self) -> impl Iterator<Item = &Work> {
        self.works.values()
    }

    pub fn read_document(&self, id: DocumentId) -> Result<Document, Error> {
        let info = self.documents.get(&id).ok_or(Error::MissingDocument(id))?;
        let content_path = info.path.join("content.md");
        let content =
            fs::read_to_string(&content_path).map_err(|error| Error::io(&content_path, error))?;
        Ok(Document::new(info.metadata.clone(), content))
    }

    pub fn create_document(&mut self, content: &str) -> Result<DocumentId, Error> {
        let created = Timestamp::now_local();
        let volume = Volume::from_timestamp(created).ok_or(Error::InvalidVolumeDate(created))?;
        let month_path = self
            .root
            .join("volumes")
            .join(format!("{:04}", volume.year()))
            .join(format!("{:02}", volume.month()));
        fs::create_dir_all(&month_path).map_err(|error| Error::io(&month_path, error))?;

        let id = DocumentId::new_v7();
        let destination = month_path.join(id.to_string());
        let staging = month_path.join(format!(".carta-document-{id}"));
        fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
        let mut guard = CleanupDirectory::new(staging.clone());

        let metadata = DocumentMetadata::new(id, created);
        write_document_metadata(&staging.join("meta.json"), &metadata)?;
        fs::write(staging.join("content.md"), content.as_bytes())
            .map_err(|error| Error::io(staging.join("content.md"), error))?;
        fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
        guard.disarm();

        self.documents.insert(
            id,
            DocumentInfo {
                metadata,
                volume,
                path: destination,
            },
        );
        Ok(id)
    }

    pub fn work(&self, id: WorkId) -> Option<&Work> {
        self.works.get(&id)
    }

    pub fn create_work(
        &mut self,
        title: String,
        documents: Vec<DocumentId>,
    ) -> Result<WorkId, Error> {
        let id = WorkId::new_v7();
        for document in &documents {
            if !self.documents.contains_key(document) {
                return Err(Error::DanglingDocumentReference {
                    work: id,
                    document: *document,
                });
            }
        }

        let works_path = self.root.join("works");
        let destination = works_path.join(id.to_string());
        let staging = works_path.join(format!(".carta-work-{id}"));
        fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
        let mut guard = CleanupDirectory::new(staging.clone());

        let metadata = WorkMetadata::new(id, Timestamp::now_local(), title, documents);
        write_work_metadata(&staging.join("work.json"), &metadata)?;
        fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
        guard.disarm();

        self.works.insert(
            id,
            Work {
                metadata,
                path: destination,
            },
        );
        Ok(id)
    }
}

fn write_archive_metadata(path: &Path, metadata: &ArchiveMetadata) -> Result<(), Error> {
    let mut writer = create_writer(path)?;
    metadata
        .write_to(&mut writer)
        .map_err(|error| Error::format(path, error))?;
    writer.flush().map_err(|error| Error::io(path, error))
}

fn write_document_metadata(path: &Path, metadata: &DocumentMetadata) -> Result<(), Error> {
    let mut writer = create_writer(path)?;
    metadata
        .write_to(&mut writer)
        .map_err(|error| Error::format(path, error))?;
    writer.flush().map_err(|error| Error::io(path, error))
}

fn write_work_metadata(path: &Path, metadata: &WorkMetadata) -> Result<(), Error> {
    let mut writer = create_writer(path)?;
    metadata
        .write_to(&mut writer)
        .map_err(|error| Error::format(path, error))?;
    writer.flush().map_err(|error| Error::io(path, error))
}

fn create_writer(path: &Path) -> Result<BufWriter<File>, Error> {
    File::create(path)
        .map(BufWriter::new)
        .map_err(|error| Error::io(path, error))
}

struct CleanupDirectory {
    path: PathBuf,
    armed: bool,
}

impl CleanupDirectory {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CleanupDirectory {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
