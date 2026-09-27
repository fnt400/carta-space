use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

use carta_format::{
    work_title_key, ArchiveMetadata, DocumentId, DocumentMetadata, Timestamp, WorkId, WorkMetadata,
    MIMETYPE,
};

use crate::validation::{scan_archive, scan_archive_with_recovery_documents};
use crate::{
    extract_markdown_links, Backlink, CartaLinkTarget, Document, DocumentInfo, Error,
    LinkResolution, MarkdownLink, SearchResult, ValidationErrors, Volume, Work, WorkProjection,
};

#[derive(Debug)]
pub struct Archive {
    pub(crate) root: PathBuf,
    metadata: ArchiveMetadata,
    pub(crate) documents: BTreeMap<DocumentId, DocumentInfo>,
    pub(crate) works: BTreeMap<WorkId, Work>,
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

        crate::history::create_initial_checkpoint(&staging)?;

        scan_archive(&guard.path)?;
        fs::rename(&guard.path, destination).map_err(|error| Error::io(destination, error))?;
        guard.disarm();
        Self::open(destination)
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let root = path.as_ref().to_path_buf();
        crate::package::cleanup_registered_temporaries(&root)?;
        crate::transaction::recover(&root)?;
        recover_creation_staging(&root)?;
        recover_git_omitted_empty_roots(&root)?;
        let scanned = scan_archive_with_recovery_documents(
            &root,
            crate::conflict::missing_document_infos(&root)?,
        )?;
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
        self.ensure_document_current(info)?;
        let content_path = info.path.join("content.md");
        let content =
            fs::read_to_string(&content_path).map_err(|error| Error::io(&content_path, error))?;
        Ok(Document::new(info.metadata.clone(), content))
    }

    pub fn create_document(&mut self, content: &str) -> Result<DocumentId, Error> {
        let created = Timestamp::now_local();
        let id = DocumentId::new_v7();
        self.create_document_with(id, created, content)?;
        Ok(id)
    }

    pub(crate) fn create_document_with(
        &mut self,
        id: DocumentId,
        created: Timestamp,
        content: &str,
    ) -> Result<PathBuf, Error> {
        let volume = Volume::from_timestamp(created).ok_or(Error::InvalidVolumeDate(created))?;
        let month_path = self
            .root
            .join("volumes")
            .join(format!("{:04}", volume.year()))
            .join(format!("{:02}", volume.month()));
        fs::create_dir_all(&month_path).map_err(|error| Error::io(&month_path, error))?;

        let destination = month_path.join(id.to_string());
        let staging = month_path.join(format!(".carta-document-{id}"));
        fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
        let mut guard = CleanupDirectory::new(staging.clone());

        let metadata = DocumentMetadata::new(id, created);
        write_document_metadata(&staging.join("meta.json"), &metadata)?;
        let content = normalize_line_endings(content);
        fs::write(staging.join("content.md"), content.as_bytes())
            .map_err(|error| Error::io(staging.join("content.md"), error))?;
        let metadata_bytes = fs::read(staging.join("meta.json"))
            .map_err(|error| Error::io(staging.join("meta.json"), error))?;
        fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
        guard.disarm();

        self.documents.insert(
            id,
            DocumentInfo {
                metadata,
                volume,
                metadata_bytes,
                content_bytes: content.into_bytes(),
                path: destination.clone(),
            },
        );
        Ok(destination)
    }

    pub fn split_document_at(
        &mut self,
        source: DocumentId,
        byte_offset: usize,
    ) -> Result<DocumentId, Error> {
        let original = self.read_document(source)?.content().to_owned();
        if !original.is_char_boundary(byte_offset) {
            return Err(Error::InvalidByteBoundary {
                document: source,
                offset: byte_offset,
            });
        }

        let source_info = self
            .documents
            .get(&source)
            .ok_or(Error::MissingDocument(source))?;
        let created = source_info.created().successor();
        let volume = Volume::from_timestamp(created).ok_or(Error::InvalidVolumeDate(created))?;
        let target = DocumentId::new_v7();
        let month_path = self
            .root
            .join("volumes")
            .join(format!("{:04}", volume.year()))
            .join(format!("{:02}", volume.month()));
        let destination = month_path.join(target.to_string());
        let staging = month_path.join(format!(".carta-document-{target}"));

        let memberships: Vec<_> = self
            .works
            .values()
            .filter(|work| work.documents().contains(&source))
            .map(Work::id)
            .collect();
        for work in &memberships {
            self.ensure_work_current(
                self.works
                    .get(work)
                    .expect("membership Work came from the current archive"),
            )?;
        }

        let mut owned = vec![
            info_relative(&self.root, &destination)?,
            info_relative(&self.root, &staging)?,
            info_relative(&self.root, &source_info.path.join("content.md"))?,
        ];
        for work in &memberships {
            owned.push(info_relative(
                &self.root,
                &self
                    .works
                    .get(work)
                    .expect("membership Work came from the current archive")
                    .path
                    .join("work.json"),
            )?);
        }

        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before Split Document",
            owned,
        )?;
        let metadata = DocumentMetadata::new(target, created);
        let (before, after) = original.split_at(byte_offset);
        let operation = (|| {
            self.edit_document(source, before)?;

            fs::create_dir_all(&month_path).map_err(|error| Error::io(&month_path, error))?;
            fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
            let mut guard = CleanupDirectory::new(staging.clone());
            write_document_metadata(&staging.join("meta.json"), &metadata)?;
            fs::write(staging.join("content.md"), after.as_bytes())
                .map_err(|error| Error::io(staging.join("content.md"), error))?;
            let metadata_bytes = fs::read(staging.join("meta.json"))
                .map_err(|error| Error::io(staging.join("meta.json"), error))?;

            for work in &memberships {
                let current = self.works.get(work).ok_or(Error::MissingWork(*work))?;
                let mut documents = current.documents().to_vec();
                let index = documents
                    .iter()
                    .position(|document| *document == source)
                    .ok_or(Error::DocumentNotInWork {
                        work: *work,
                        document: source,
                    })?;
                documents.insert(index + 1, target);
                let work_metadata = current
                    .metadata
                    .with_documents(documents)
                    .map_err(|error| Error::format(current.path.join("work.json"), error))?;
                self.replace_work(*work, work_metadata)?;
            }

            fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
            guard.disarm();
            sync_created_directory(&destination)?;
            Ok(metadata_bytes)
        })();

        match operation {
            Ok(metadata_bytes) => {
                self.documents.insert(
                    target,
                    DocumentInfo {
                        metadata,
                        volume,
                        path: destination,
                        metadata_bytes,
                        content_bytes: after.as_bytes().to_vec(),
                    },
                );
                transaction.commit()?;
                Ok(target)
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub fn refresh(&mut self) -> Result<(), Error> {
        crate::transaction::recover(&self.root)?;
        recover_creation_staging(&self.root)?;
        let scanned = scan_archive_with_recovery_documents(
            &self.root,
            crate::conflict::missing_document_infos(&self.root)?,
        )?;
        self.metadata = scanned.metadata;
        self.documents = scanned.documents;
        self.works = scanned.works;
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), Error> {
        self.refresh()
    }

    pub fn edit_document(&mut self, id: DocumentId, content: &str) -> Result<(), Error> {
        let info = self.documents.get(&id).ok_or(Error::MissingDocument(id))?;
        let path = info.path.join("content.md");
        let content = normalize_line_endings(content).into_bytes();
        if !info.path.join("meta.json").is_file() {
            let conflict =
                self.preserve_document_conflict(id, &content, &info.content_bytes, true)?;
            return Err(Error::ConflictPreserved(conflict));
        }
        ensure_unchanged(&info.path.join("meta.json"), &info.metadata_bytes)?;
        let external = match fs::read(&path) {
            Ok(external) => external,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let conflict =
                    self.preserve_document_conflict(id, &content, &info.content_bytes, true)?;
                return Err(Error::ConflictPreserved(conflict));
            }
            Err(error) => return Err(Error::io(&path, error)),
        };
        if external != info.content_bytes {
            let conflict = self.preserve_document_conflict(id, &content, &external, false)?;
            return Err(Error::ConflictPreserved(conflict));
        }
        atomic_replace(&path, &content)?;
        self.documents
            .get_mut(&id)
            .expect("document was checked above")
            .content_bytes = content;
        Ok(())
    }

    pub fn duplicate_document(&mut self, source: DocumentId) -> Result<DocumentId, Error> {
        let content = self.read_document(source)?.content().to_owned();
        self.create_document(&content)
    }

    pub fn import_document_bytes(&mut self, bytes: &[u8]) -> Result<DocumentId, Error> {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        self.create_document(std::str::from_utf8(bytes)?)
    }

    pub fn export_document_markdown(&self, id: DocumentId) -> Result<String, Error> {
        Ok(self.read_document(id)?.content().to_owned())
    }

    pub fn export_work_markdown(&self, id: WorkId) -> Result<String, Error> {
        let work = self.works.get(&id).ok_or(Error::MissingWork(id))?;
        self.ensure_work_current(work)?;
        let mut output = String::new();
        for document in work.documents() {
            output.push_str(self.read_document(*document)?.content());
        }
        Ok(output)
    }

    pub fn export_document_markdown_file(
        &self,
        id: DocumentId,
        destination: impl AsRef<Path>,
    ) -> Result<(), Error> {
        self.export_markdown_file(&self.export_document_markdown(id)?, destination.as_ref())
    }

    pub fn export_work_markdown_file(
        &self,
        id: WorkId,
        destination: impl AsRef<Path>,
    ) -> Result<(), Error> {
        self.export_markdown_file(&self.export_work_markdown(id)?, destination.as_ref())
    }

    fn export_markdown_file(&self, markdown: &str, destination: &Path) -> Result<(), Error> {
        use crate::package::{
            replace_destination, safe_export_destination, sync_parent, temporary_sibling,
            TemporaryFile,
        };
        let destination = safe_export_destination(&self.root, destination)?;
        let temporary = temporary_sibling(&destination, "markdown")?;
        let mut guard = TemporaryFile::register(&self.root, temporary.clone())?;
        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| Error::io(&temporary, error))?;
        file.write_all(markdown.as_bytes())
            .map_err(|error| Error::io(&temporary, error))?;
        file.sync_all()
            .map_err(|error| Error::io(&temporary, error))?;
        if std::env::var_os("CARTA_TEST_EXPORT_CRASH_AFTER_TEMP_WRITE")
            .is_some_and(|path| Path::new(&path) == self.root)
        {
            std::mem::forget(guard);
            return Err(Error::InvalidPackage(
                "injected crash after external temporary write".to_owned(),
            ));
        }
        replace_destination(&temporary, &destination)?;
        guard.finish()?;
        sync_parent(&destination)
    }

    pub fn document_links(&self, id: DocumentId) -> Result<Vec<MarkdownLink>, Error> {
        Ok(extract_markdown_links(self.read_document(id)?.content()))
    }

    pub fn document_link_at(
        &self,
        id: DocumentId,
        byte_offset: usize,
    ) -> Result<Option<MarkdownLink>, Error> {
        let document = self.read_document(id)?;
        Ok(crate::link_at_byte_offset(document.content(), byte_offset))
    }

    pub fn resolve_link(&self, link: &MarkdownLink) -> Option<LinkResolution> {
        match link.carta_target()? {
            CartaLinkTarget::Document(id) if self.documents.contains_key(&id) => {
                Some(LinkResolution::Document(id))
            }
            CartaLinkTarget::Work(id) if self.works.contains_key(&id) => {
                Some(LinkResolution::Work(id))
            }
            target => Some(LinkResolution::Unresolved(target)),
        }
    }

    pub fn backlinks(&self, target: CartaLinkTarget) -> Result<Vec<Backlink>, Error> {
        let mut backlinks = Vec::new();
        for info in self.documents.values() {
            for link in self.document_links(info.id())? {
                if link.carta_target() == Some(target) {
                    backlinks.push(Backlink::new(info.id(), link));
                }
            }
        }
        Ok(backlinks)
    }

    pub fn search(&self, query: &str) -> Result<Vec<SearchResult>, Error> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        if query.contains(['\r', '\n']) {
            return Err(Error::MultilineSearchQuery);
        }
        let mut results = Vec::new();
        for info in self.documents.values() {
            let document = self.read_document(info.id())?;
            if let Some(occurrence) = crate::retrieval::literal_match(document.content(), query) {
                results.push(SearchResult::new(
                    info.id(),
                    info.created(),
                    document.derived_label(),
                    occurrence,
                    document.content(),
                ));
            }
        }
        results.sort_by_key(|result| std::cmp::Reverse((result.created(), result.document())));
        Ok(results)
    }

    pub fn new_linked_document(
        &mut self,
        source: DocumentId,
        byte_offset: usize,
        label: &str,
    ) -> Result<DocumentId, Error> {
        self.new_linked_document_with(source, byte_offset, label, |_, _| Ok(()))
    }

    fn new_linked_document_with(
        &mut self,
        source: DocumentId,
        byte_offset: usize,
        label: &str,
        before_publish: impl FnOnce(&mut Self, &Path) -> Result<(), Error>,
    ) -> Result<DocumentId, Error> {
        let original = self.read_document(source)?.content().to_owned();
        let mut content = original.clone();
        if !content.is_char_boundary(byte_offset) {
            return Err(Error::InvalidByteBoundary {
                document: source,
                offset: byte_offset,
            });
        }

        let created = Timestamp::now_local();
        let volume = Volume::from_timestamp(created).ok_or(Error::InvalidVolumeDate(created))?;
        let month_path = self
            .root
            .join("volumes")
            .join(format!("{:04}", volume.year()))
            .join(format!("{:02}", volume.month()));
        let target = DocumentId::new_v7();
        let destination = month_path.join(target.to_string());
        let staging = month_path.join(format!(".carta-linked-{target}"));
        let destination_relative = destination
            .strip_prefix(&self.root)
            .expect("Document destination is inside Archive")
            .to_path_buf();
        let staging_relative = staging
            .strip_prefix(&self.root)
            .expect("Document staging is inside Archive")
            .to_path_buf();
        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before New Linked Document",
            [
                destination_relative,
                staging_relative,
                info_relative(&self.root, &self.documents[&source].path.join("content.md"))?,
            ],
        )?;
        let metadata = DocumentMetadata::new(target, created);
        let operation = (|| {
            fs::create_dir_all(&month_path).map_err(|error| Error::io(&month_path, error))?;
            fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
            let mut guard = CleanupLinkedStaging::new(staging.clone());
            write_document_metadata(&staging.join("meta.json"), &metadata)?;
            fs::write(staging.join("content.md"), b"")
                .map_err(|error| Error::io(staging.join("content.md"), error))?;
            let metadata_bytes = fs::read(staging.join("meta.json"))
                .map_err(|error| Error::io(staging.join("meta.json"), error))?;
            let link = format!(
                "[{}](carta:doc:{target})",
                label
                    .replace('\\', "\\\\")
                    .replace('[', "\\[")
                    .replace(']', "\\]")
            );
            content.insert_str(byte_offset, &link);
            self.edit_document(source, &content)?;
            before_publish(self, &destination)?;
            fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
            guard.disarm();
            sync_created_directory(&destination)?;
            Ok(metadata_bytes)
        })();
        match operation {
            Ok(metadata_bytes) => {
                self.documents.insert(
                    target,
                    DocumentInfo {
                        metadata,
                        volume,
                        path: destination,
                        metadata_bytes,
                        content_bytes: Vec::new(),
                    },
                );
                transaction.commit()?;
                Ok(target)
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub fn create_document_in_work_after(
        &mut self,
        work: WorkId,
        after: Option<DocumentId>,
    ) -> Result<DocumentId, Error> {
        let id = DocumentId::new_v7();
        let created = Timestamp::now_local();
        let volume = Volume::from_timestamp(created).ok_or(Error::InvalidVolumeDate(created))?;
        let relative = PathBuf::from("volumes")
            .join(format!("{:04}", volume.year()))
            .join(format!("{:02}", volume.month()))
            .join(id.to_string());
        let transaction = crate::transaction::Transaction::begin(
            &self.root,
            "Saved state before provisional Work Document",
            [
                relative.clone(),
                info_relative(
                    &self.root,
                    &self
                        .works
                        .get(&work)
                        .ok_or(Error::MissingWork(work))?
                        .path
                        .join("work.json"),
                )?,
            ],
        )?;
        let result = (|| {
            let current = self.works.get(&work).ok_or(Error::MissingWork(work))?;
            self.ensure_work_current(current)?;
            let mut documents = current.documents().to_vec();
            let index = match after {
                Some(after) => documents
                    .iter()
                    .position(|candidate| *candidate == after)
                    .map(|index| index + 1)
                    .ok_or(Error::DocumentNotInWork {
                        work,
                        document: after,
                    })?,
                None => 0,
            };
            let destination = self.root.join(&relative);
            fs::create_dir_all(destination.parent().expect("destination has parent"))
                .map_err(|error| Error::io(destination.parent().unwrap(), error))?;
            fs::create_dir(&destination).map_err(|error| Error::io(&destination, error))?;
            write_document_metadata(
                &destination.join("meta.json"),
                &DocumentMetadata::new(id, created),
            )?;
            fs::write(destination.join("content.md"), b"")
                .map_err(|error| Error::io(destination.join("content.md"), error))?;
            documents.insert(index, id);
            let metadata = current
                .metadata
                .with_documents(documents)
                .map_err(|error| Error::format(current.path.join("work.json"), error))?;
            self.replace_work(work, metadata)?;
            sync_created_directory(&destination)?;
            Ok(id)
        })();
        match result {
            Ok(id) => {
                transaction.commit()?;
                self.refresh()?;
                Ok(id)
            }
            Err(error) => {
                let error = transaction.rollback_error(error);
                self.refresh()?;
                Err(error)
            }
        }
    }

    pub fn work(&self, id: WorkId) -> Option<&Work> {
        self.works.get(&id)
    }

    pub fn work_title_conflict(&self, title: &str, except: Option<WorkId>) -> Option<WorkId> {
        let key = work_title_key(title);
        self.works
            .values()
            .find(|work| Some(work.id()) != except && work_title_key(work.title()) == key)
            .map(Work::id)
    }

    pub fn create_work(
        &mut self,
        title: String,
        documents: Vec<DocumentId>,
    ) -> Result<WorkId, Error> {
        let id = WorkId::new_v7();
        let title_key = work_title_key(&title);
        if let Some(existing) = self
            .works
            .values()
            .find(|work| work_title_key(work.title()) == title_key)
        {
            return Err(Error::WorkTitleConflict {
                title,
                existing: existing.id(),
            });
        }
        let mut unique_documents = HashSet::new();
        for document in &documents {
            if !unique_documents.insert(*document) {
                return Err(Error::DuplicateWorkDocument(*document));
            }
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

        let metadata = WorkMetadata::new(id, Timestamp::now_local(), title, documents)
            .map_err(|error| Error::format(staging.join("work.json"), error))?;
        write_work_metadata(&staging.join("work.json"), &metadata)?;
        let metadata_bytes = fs::read(staging.join("work.json"))
            .map_err(|error| Error::io(staging.join("work.json"), error))?;
        fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
        guard.disarm();

        self.works.insert(
            id,
            Work {
                metadata,
                metadata_bytes,
                path: destination,
            },
        );
        Ok(id)
    }

    pub fn create_empty_work(&mut self, title: String) -> Result<WorkId, Error> {
        self.create_work(title, Vec::new())
    }

    pub fn set_work_color(&mut self, id: WorkId, color: Option<String>) -> Result<(), Error> {
        let metadata = self
            .works
            .get(&id)
            .ok_or(Error::MissingWork(id))?
            .metadata
            .with_color(color);
        self.replace_work(id, metadata)
    }

    pub fn rename_work(&mut self, id: WorkId, title: String) -> Result<(), Error> {
        let title_key = work_title_key(&title);
        if let Some(existing) = self
            .works
            .values()
            .find(|work| work.id() != id && work_title_key(work.title()) == title_key)
        {
            return Err(Error::WorkTitleConflict {
                title,
                existing: existing.id(),
            });
        }
        let metadata = self
            .works
            .get(&id)
            .ok_or(Error::MissingWork(id))?
            .metadata
            .with_title(title);
        self.replace_work(id, metadata)
    }

    pub fn add_document_to_work(
        &mut self,
        work: WorkId,
        document: DocumentId,
    ) -> Result<(), Error> {
        if !self.documents.contains_key(&document) {
            return Err(Error::MissingDocument(document));
        }
        let current = self.works.get(&work).ok_or(Error::MissingWork(work))?;
        if current.documents().contains(&document) {
            return Err(Error::DuplicateWorkDocument(document));
        }
        let mut documents = current.documents().to_vec();
        documents.push(document);
        let metadata = current
            .metadata
            .with_documents(documents)
            .map_err(|error| Error::format(current.path.join("work.json"), error))?;
        self.replace_work(work, metadata)
    }

    pub fn remove_document_from_work(
        &mut self,
        work: WorkId,
        document: DocumentId,
    ) -> Result<(), Error> {
        let current = self.works.get(&work).ok_or(Error::MissingWork(work))?;
        let mut documents = current.documents().to_vec();
        let Some(index) = documents
            .iter()
            .position(|candidate| *candidate == document)
        else {
            return Err(Error::DocumentNotInWork { work, document });
        };
        documents.remove(index);
        let metadata = current
            .metadata
            .with_documents(documents)
            .map_err(|error| Error::format(current.path.join("work.json"), error))?;
        self.replace_work(work, metadata)
    }

    pub fn move_document_earlier(
        &mut self,
        work: WorkId,
        document: DocumentId,
    ) -> Result<bool, Error> {
        self.move_document_by(work, document, |index, _| index.checked_sub(1))
    }

    pub fn move_document_later(
        &mut self,
        work: WorkId,
        document: DocumentId,
    ) -> Result<bool, Error> {
        self.move_document_by(work, document, |index, len| {
            (index + 1 < len).then_some(index + 1)
        })
    }

    pub fn move_document_after(
        &mut self,
        work: WorkId,
        document: DocumentId,
        after: Option<DocumentId>,
    ) -> Result<(), Error> {
        let current = self.works.get(&work).ok_or(Error::MissingWork(work))?;
        let mut documents = current.documents().to_vec();
        let Some(index) = documents
            .iter()
            .position(|candidate| *candidate == document)
        else {
            return Err(Error::DocumentNotInWork { work, document });
        };
        documents.remove(index);
        let destination = match after {
            None => 0,
            Some(after) => documents
                .iter()
                .position(|candidate| *candidate == after)
                .map(|index| index + 1)
                .ok_or(Error::DocumentNotInWork {
                    work,
                    document: after,
                })?,
        };
        documents.insert(destination, document);
        let metadata = current
            .metadata
            .with_documents(documents)
            .map_err(|error| Error::format(current.path.join("work.json"), error))?;
        self.replace_work(work, metadata)
    }

    pub fn memberships(&self, document: DocumentId) -> Result<Vec<WorkId>, Error> {
        if !self.documents.contains_key(&document) {
            return Err(Error::MissingDocument(document));
        }
        Ok(self
            .works
            .values()
            .filter(|work| work.documents().contains(&document))
            .map(Work::id)
            .collect())
    }

    pub fn chronological_month(&self, volume: Volume) -> Vec<DocumentId> {
        let mut documents: Vec<_> = self
            .documents
            .values()
            .filter(|document| document.volume() == volume)
            .collect();
        documents.sort_by_key(|document| (document.created(), document.id()));
        documents.into_iter().map(DocumentInfo::id).collect()
    }

    pub fn work_projection(&self, id: WorkId) -> Result<WorkProjection, Error> {
        let work = self.works.get(&id).ok_or(Error::MissingWork(id))?;
        Ok(WorkProjection::new(id, work.documents()))
    }

    pub(crate) fn replace_work(&mut self, id: WorkId, metadata: WorkMetadata) -> Result<(), Error> {
        let work = self.works.get(&id).ok_or(Error::MissingWork(id))?;
        let path = work.path.join("work.json");
        let bytes = serialize_work_metadata(&path, &metadata)?;
        let external = match fs::read(&path) {
            Ok(external) => external,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let conflict =
                    self.preserve_work_conflict(id, &bytes, &work.metadata_bytes, true)?;
                return Err(Error::ConflictPreserved(conflict));
            }
            Err(error) => return Err(Error::io(&path, error)),
        };
        if external != work.metadata_bytes {
            let conflict = self.preserve_work_conflict(id, &bytes, &external, false)?;
            return Err(Error::ConflictPreserved(conflict));
        }
        atomic_replace(&path, &bytes)?;
        let work = self.works.get_mut(&id).expect("work was checked above");
        work.metadata = metadata;
        work.metadata_bytes = bytes;
        Ok(())
    }

    fn move_document_by(
        &mut self,
        work: WorkId,
        document: DocumentId,
        destination: impl FnOnce(usize, usize) -> Option<usize>,
    ) -> Result<bool, Error> {
        let current = self.works.get(&work).ok_or(Error::MissingWork(work))?;
        let mut documents = current.documents().to_vec();
        let Some(index) = documents
            .iter()
            .position(|candidate| *candidate == document)
        else {
            return Err(Error::DocumentNotInWork { work, document });
        };
        let Some(destination) = destination(index, documents.len()) else {
            return Ok(false);
        };
        documents.swap(index, destination);
        let metadata = current
            .metadata
            .with_documents(documents)
            .map_err(|error| Error::format(current.path.join("work.json"), error))?;
        self.replace_work(work, metadata)?;
        Ok(true)
    }

    pub(crate) fn ensure_document_current(&self, info: &DocumentInfo) -> Result<(), Error> {
        ensure_unchanged(&info.path.join("meta.json"), &info.metadata_bytes)?;
        ensure_unchanged(&info.path.join("content.md"), &info.content_bytes)
    }

    pub(crate) fn ensure_work_current(&self, work: &Work) -> Result<(), Error> {
        ensure_unchanged(&work.path.join("work.json"), &work.metadata_bytes)
    }
}

fn recover_git_omitted_empty_roots(root: &Path) -> Result<(), Error> {
    for name in ["volumes", "works"] {
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(&path, error)),
        }

        let output = crate::history::git_output(
            root,
            "inspect missing Archive root",
            &["ls-tree", "-r", "--name-only", "HEAD", "--", name],
        )?;
        if output.stdout.is_empty() {
            fs::create_dir(&path).map_err(|error| Error::io(&path, error))?;
        }
    }
    Ok(())
}

fn ensure_unchanged(path: &Path, expected: &[u8]) -> Result<(), Error> {
    let current = fs::read(path).map_err(|error| Error::io(path, error))?;
    if current != expected {
        return Err(Error::ExternalChange(path.to_path_buf()));
    }
    Ok(())
}

fn info_relative(root: &Path, path: &Path) -> Result<PathBuf, Error> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|_| Error::PathOutsideArchive(path.to_path_buf()))
}

pub(crate) fn serialize_work_metadata(
    path: &Path,
    metadata: &WorkMetadata,
) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    metadata
        .write_to(&mut bytes)
        .map_err(|error| Error::format(path, error))?;
    Ok(bytes)
}

pub(crate) fn atomic_replace(path: &Path, content: &[u8]) -> Result<(), Error> {
    let parent = path
        .parent()
        .expect("canonical files always have a parent directory");
    let name = path
        .file_name()
        .expect("canonical files always have a file name")
        .to_string_lossy();
    let temporary = parent.join(format!(".carta-tmp-{name}-{}", DocumentId::new_v7()));
    let mut guard = CleanupFile::new(temporary.clone());
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| Error::io(&temporary, error))?;
    file.write_all(content)
        .map_err(|error| Error::io(&temporary, error))?;
    file.sync_all()
        .map_err(|error| Error::io(&temporary, error))?;
    fs::rename(&temporary, path).map_err(|error| Error::io(path, error))?;
    guard.disarm();
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| Error::io(parent, error))
}

fn recover_creation_staging(root: &Path) -> Result<(), Error> {
    let volumes = root.join("volumes");
    if volumes.is_dir() {
        for year in directory_paths(&volumes)? {
            if !year.is_dir() {
                continue;
            }
            for month in directory_paths(&year)? {
                if !month.is_dir() {
                    continue;
                }
                let volume = month
                    .file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|month| month.parse::<u8>().ok())
                    .and_then(|month| {
                        year.file_name()
                            .and_then(|name| name.to_str())
                            .and_then(|year| year.parse::<u16>().ok())
                            .and_then(|year| Volume::new(year, month))
                    });
                let Some(volume) = volume else { continue };
                for staging in directory_paths(&month)? {
                    if let Some(id) = staging_id::<DocumentId>(&staging, ".carta-linked-") {
                        let destination = month.join(id.to_string());
                        if !destination.exists() && recoverable_document(&staging, id, volume) {
                            if archive_contains_link(&volumes, id)? {
                                fs::rename(&staging, &destination)
                                    .map_err(|error| Error::io(&destination, error))?;
                            } else {
                                remove_abandoned_linked_staging(&staging)?;
                            }
                        }
                        continue;
                    }
                    let Some(id) = staging_id::<DocumentId>(&staging, ".carta-document-") else {
                        continue;
                    };
                    let destination = month.join(id.to_string());
                    if destination.exists() || !recoverable_document(&staging, id, volume) {
                        continue;
                    }
                    fs::rename(&staging, &destination)
                        .map_err(|error| Error::io(&destination, error))?;
                }
            }
        }
    }

    let works = root.join("works");
    if works.is_dir() {
        let documents = current_document_ids(&volumes)?;
        let mut titles = current_work_titles(&works)?;
        for staging in directory_paths(&works)? {
            let Some(id) = staging_id::<WorkId>(&staging, ".carta-work-") else {
                continue;
            };
            let destination = works.join(id.to_string());
            let Some(metadata) = recoverable_work(&staging, id) else {
                continue;
            };
            let title = work_title_key(metadata.title());
            if destination.exists()
                || metadata
                    .documents()
                    .iter()
                    .any(|document| !documents.contains(document))
                || titles.contains(&title)
            {
                continue;
            }
            fs::rename(&staging, &destination).map_err(|error| Error::io(&destination, error))?;
            titles.insert(title);
        }
    }
    Ok(())
}

fn directory_paths(path: &Path) -> Result<Vec<PathBuf>, Error> {
    let entries = fs::read_dir(path).map_err(|error| Error::io(path, error))?;
    let mut paths = Vec::new();
    for entry in entries {
        paths.push(entry.map_err(|error| Error::io(path, error))?.path());
    }
    Ok(paths)
}

fn staging_id<T: FromStr>(path: &Path, prefix: &str) -> Option<T> {
    path.file_name()?
        .to_str()?
        .strip_prefix(prefix)?
        .parse()
        .ok()
}

fn recoverable_document(path: &Path, id: DocumentId, volume: Volume) -> bool {
    if !path.is_dir() {
        return false;
    }
    let metadata = File::open(path.join("meta.json"))
        .map(BufReader::new)
        .map_err(carta_format::FormatError::from)
        .and_then(DocumentMetadata::read_from);
    let content = fs::read_to_string(path.join("content.md"));
    matches!(metadata, Ok(metadata) if metadata.id() == id && Volume::from_timestamp(metadata.created()) == Some(volume))
        && matches!(content, Ok(content) if !content.contains('\r'))
}

fn recoverable_work(path: &Path, id: WorkId) -> Option<WorkMetadata> {
    if !path.is_dir() {
        return None;
    }
    File::open(path.join("work.json"))
        .map(BufReader::new)
        .map_err(carta_format::FormatError::from)
        .and_then(WorkMetadata::read_from)
        .ok()
        .filter(|metadata| metadata.id() == id)
}

fn current_document_ids(volumes: &Path) -> Result<HashSet<DocumentId>, Error> {
    let mut ids = HashSet::new();
    for year in directory_paths(volumes)? {
        if !year.is_dir() {
            continue;
        }
        for month in directory_paths(&year)? {
            if !month.is_dir() {
                continue;
            }
            for document in directory_paths(&month)? {
                if document.is_dir() {
                    if let Some(id) = staging_id(&document, "") {
                        ids.insert(id);
                    }
                }
            }
        }
    }
    Ok(ids)
}

fn archive_contains_link(volumes: &Path, target: DocumentId) -> Result<bool, Error> {
    let destination = format!("carta:doc:{target}");
    for year in directory_paths(volumes)? {
        if !year.is_dir() {
            continue;
        }
        for month in directory_paths(&year)? {
            if !month.is_dir() {
                continue;
            }
            for document in directory_paths(&month)? {
                if staging_id::<DocumentId>(&document, "").is_some()
                    && fs::read_to_string(document.join("content.md"))
                        .is_ok_and(|content| content.contains(&destination))
                {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

fn remove_abandoned_linked_staging(path: &Path) -> Result<(), Error> {
    let entries = directory_paths(path)?;
    if entries.len() != 2
        || !entries.iter().all(|entry| {
            matches!(
                entry.file_name().and_then(|name| name.to_str()),
                Some("content.md" | "meta.json")
            )
        })
        || !fs::read(path.join("content.md")).is_ok_and(|content| content.is_empty())
    {
        return Ok(());
    }
    fs::remove_file(path.join("content.md"))
        .map_err(|error| Error::io(path.join("content.md"), error))?;
    fs::remove_file(path.join("meta.json"))
        .map_err(|error| Error::io(path.join("meta.json"), error))?;
    fs::remove_dir(path).map_err(|error| Error::io(path, error))
}

fn current_work_titles(works: &Path) -> Result<HashSet<String>, Error> {
    let mut titles = HashSet::new();
    for path in directory_paths(works)? {
        if staging_id::<WorkId>(&path, "").is_none() {
            continue;
        }
        if let Ok(metadata) = File::open(path.join("work.json"))
            .map(BufReader::new)
            .map_err(carta_format::FormatError::from)
            .and_then(WorkMetadata::read_from)
        {
            titles.insert(work_title_key(metadata.title()));
        }
    }
    Ok(titles)
}

fn normalize_line_endings(content: &str) -> String {
    content.replace("\r\n", "\n").replace('\r', "\n")
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

fn sync_created_directory(path: &Path) -> Result<(), Error> {
    for name in ["meta.json", "content.md"] {
        let file = path.join(name);
        File::open(&file)
            .and_then(|file| file.sync_all())
            .map_err(|error| Error::io(&file, error))?;
    }
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| Error::io(path, error))?;
    let parent = path.parent().expect("created directory has a parent");
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| Error::io(parent, error))
}

struct CleanupDirectory {
    path: PathBuf,
    armed: bool,
}

struct CleanupFile {
    path: PathBuf,
    armed: bool,
}

struct CleanupLinkedStaging {
    path: PathBuf,
    armed: bool,
}

impl CleanupLinkedStaging {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CleanupLinkedStaging {
    fn drop(&mut self) {
        if self.armed {
            let _ = remove_abandoned_linked_staging(&self.path);
        }
    }
}

impl CleanupFile {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CleanupFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_document_rolls_back_source_if_target_publication_fails() {
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        let source = archive.create_document("source").unwrap();

        let error = archive
            .new_linked_document_with(source, 0, "Continue", |_archive, destination| {
                fs::create_dir(destination).map_err(|error| Error::io(destination, error))?;
                fs::write(destination.join("block"), b"block")
                    .map_err(|error| Error::io(destination.join("block"), error))
            })
            .unwrap_err();

        assert!(matches!(error, Error::Io { .. }));
        assert_eq!(archive.documents.len(), 1);
        assert_eq!(
            fs::read_to_string(
                archive
                    .documents
                    .get(&source)
                    .unwrap()
                    .path
                    .join("content.md")
            )
            .unwrap(),
            "source"
        );
        assert!(!archive.root().join(".git/carta-transaction.json").exists());
        Archive::validate(archive.root()).unwrap();
    }
}
