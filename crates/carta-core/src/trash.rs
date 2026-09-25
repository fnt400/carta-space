use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::str::FromStr;

use carta_format::{work_title_key, DocumentId, Timestamp, WorkId, WorkMetadata};

use crate::history::{
    git_command, git_output, read_blob, read_document_at, require_checkpoint,
    write_restored_document,
};
use crate::{
    Archive, Backlink, CartaLinkTarget, Checkpoint, CheckpointId, CheckpointKind, Document, Error,
    Volume,
};

/// Scope of the on-device Wipe guarantee.
pub const WIPE_GUARANTEE: &str = "Removes this Document's canonical paths and exclusively owned content from all retained local Git refs, reflogs, unreachable Git objects, and Carta-reserved local artifacts. It does not cover external backups, exports, clones, snapshots, copied files, or physical storage remnants.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkMembership {
    id: WorkId,
    title: String,
}

impl WorkMembership {
    pub fn id(&self) -> WorkId {
        self.id
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentTrashImpact {
    document: DocumentId,
    memberships: Vec<WorkMembership>,
    inbound_links: Vec<Backlink>,
}

impl DocumentTrashImpact {
    pub fn document(&self) -> DocumentId {
        self.document
    }

    pub fn memberships(&self) -> &[WorkMembership] {
        &self.memberships
    }

    pub fn inbound_links(&self) -> &[Backlink] {
        &self.inbound_links
    }
}

#[derive(Debug, Clone)]
pub struct TrashedDocument {
    id: DocumentId,
    created: Timestamp,
    volume: Volume,
    label: String,
}

impl TrashedDocument {
    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn volume(&self) -> Volume {
        self.volume
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Debug, Clone)]
pub struct TrashedWork {
    id: WorkId,
    created: Timestamp,
    title: String,
    documents: Vec<DocumentId>,
}

impl TrashedWork {
    pub fn id(&self) -> WorkId {
        self.id
    }

    pub fn created(&self) -> Timestamp {
        self.created
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn documents(&self) -> &[DocumentId] {
        &self.documents
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrashInventory {
    documents: Vec<TrashedDocument>,
    works: Vec<TrashedWork>,
}

impl TrashInventory {
    pub fn documents(&self) -> &[TrashedDocument] {
        &self.documents
    }

    pub fn works(&self) -> &[TrashedWork] {
        &self.works
    }
}

#[derive(Debug, Clone)]
pub struct WipePlan {
    document: DocumentId,
    planned_head: String,
    confirmation_token: String,
    canonical_directories: Vec<String>,
    content_objects: Vec<String>,
    retained_refs: Vec<String>,
    retained_ref_fingerprint: Vec<(String, String, String)>,
}

impl WipePlan {
    pub fn document(&self) -> DocumentId {
        self.document
    }

    /// Frontends should require the user to enter this complete token verbatim.
    pub fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }

    pub fn canonical_directories(&self) -> &[String] {
        &self.canonical_directories
    }

    pub fn retained_refs(&self) -> &[String] {
        &self.retained_refs
    }

    pub fn guarantee(&self) -> &'static str {
        WIPE_GUARANTEE
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WipeReport {
    rewritten_refs: usize,
    rewritten_commits: usize,
    removed_artifacts: usize,
}

impl WipeReport {
    pub fn rewritten_refs(&self) -> usize {
        self.rewritten_refs
    }

    pub fn rewritten_commits(&self) -> usize {
        self.rewritten_commits
    }

    pub fn removed_artifacts(&self) -> usize {
        self.removed_artifacts
    }

    pub fn guarantee(&self) -> &'static str {
        WIPE_GUARANTEE
    }
}

impl Archive {
    pub fn document_trash_impact(
        &self,
        document: DocumentId,
    ) -> Result<DocumentTrashImpact, Error> {
        if !self.documents.contains_key(&document) {
            return Err(Error::MissingDocument(document));
        }
        let memberships = self
            .works
            .values()
            .filter(|work| work.documents().contains(&document))
            .map(|work| WorkMembership {
                id: work.id(),
                title: work.title().to_owned(),
            })
            .collect();
        let inbound_links = self.backlinks(CartaLinkTarget::Document(document))?;
        Ok(DocumentTrashImpact {
            document,
            memberships,
            inbound_links,
        })
    }

    pub fn trash_document(&mut self, document: DocumentId) -> Result<Checkpoint, Error> {
        let impact = self.document_trash_impact(document)?;
        let info = self
            .documents
            .get(&document)
            .expect("impact verified the Document")
            .clone();
        self.ensure_document_current(&info)?;
        for membership in impact.memberships() {
            self.ensure_work_current(
                self.works
                    .get(&membership.id())
                    .expect("impact came from active Works"),
            )?;
        }

        let original_head = head(self.root())?;
        checkpoint_pending_state(self.root(), "Saved state immediately before Trash")?;
        let transaction =
            transaction_directory(self.root(), "trash-document", document.to_string())?;
        let saved_document = transaction.join("document");
        let mut originals = Vec::new();

        let operation = (|| {
            for membership in impact.memberships() {
                let work = self.works.get(&membership.id()).expect("active membership");
                let path = work.path.join("work.json");
                originals.push((
                    path.clone(),
                    fs::read(&path).map_err(|error| Error::io(&path, error))?,
                ));
                let documents = work
                    .documents()
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != document)
                    .collect();
                let metadata = work
                    .metadata
                    .with_documents(documents)
                    .map_err(|error| Error::format(&path, error))?;
                let bytes = crate::archive::serialize_work_metadata(&path, &metadata)?;
                crate::archive::atomic_replace(&path, &bytes)?;
            }
            fs::rename(&info.path, &saved_document)
                .map_err(|error| Error::io(&info.path, error))?;
            require_checkpoint(
                self.root(),
                CheckpointKind::Structural,
                Some("Trashed a Document and removed its Work memberships"),
            )
        })();

        match operation {
            Ok(checkpoint) => {
                let _ = fs::remove_dir_all(&transaction);
                self.refresh()?;
                Ok(checkpoint)
            }
            Err(error) => {
                let result = rollback_structural(
                    self.root(),
                    &original_head,
                    &originals,
                    &[(saved_document, info.path)],
                    &[],
                    error,
                );
                let _ = fs::remove_dir_all(&transaction);
                result?;
                unreachable!("rollback returns the operation error")
            }
        }
    }

    pub fn trash_work(&mut self, work: WorkId) -> Result<Checkpoint, Error> {
        let current = self.works.get(&work).ok_or(Error::MissingWork(work))?;
        self.ensure_work_current(current)?;
        let path = current.path.clone();
        let original_head = head(self.root())?;
        checkpoint_pending_state(self.root(), "Saved state immediately before Trash")?;
        let transaction = transaction_directory(self.root(), "trash-work", work.to_string())?;
        let saved = transaction.join("work");
        let operation = fs::rename(&path, &saved)
            .map_err(|error| Error::io(&path, error))
            .and_then(|()| {
                require_checkpoint(
                    self.root(),
                    CheckpointKind::Structural,
                    Some("Trashed a Work"),
                )
            });
        match operation {
            Ok(checkpoint) => {
                let _ = fs::remove_dir_all(&transaction);
                self.refresh()?;
                Ok(checkpoint)
            }
            Err(error) => {
                let result = rollback_structural(
                    self.root(),
                    &original_head,
                    &[],
                    &[(saved, path)],
                    &[],
                    error,
                );
                let _ = fs::remove_dir_all(&transaction);
                result?;
                unreachable!("rollback returns the operation error")
            }
        }
    }

    pub fn trash_inventory(&self) -> Result<TrashInventory, Error> {
        let commits = commits_from_head(self.root())?;
        let (historical_documents, historical_works) = historical_ids(self.root(), &commits)?;
        let mut inventory = TrashInventory::default();
        for id in historical_documents {
            if self.documents.contains_key(&id) {
                continue;
            }
            let document = latest_document(self.root(), id, &commits)?
                .ok_or(Error::DocumentNotRecoverable(id))?;
            let volume = Volume::from_timestamp(document.metadata().created())
                .ok_or(Error::InvalidVolumeDate(document.metadata().created()))?;
            inventory.documents.push(TrashedDocument {
                id,
                created: document.metadata().created(),
                volume,
                label: document.derived_label(),
            });
        }
        for id in historical_works {
            if self.works.contains_key(&id) {
                continue;
            }
            let metadata =
                latest_work(self.root(), id, &commits)?.ok_or(Error::WorkNotRecoverable(id))?;
            inventory.works.push(TrashedWork {
                id,
                created: metadata.created(),
                title: metadata.title().to_owned(),
                documents: metadata.documents().to_vec(),
            });
        }
        inventory.documents.sort_by_key(|entry| entry.id);
        inventory.works.sort_by_key(|entry| entry.id);
        Ok(inventory)
    }

    pub fn restore_trashed_document(&mut self, document: DocumentId) -> Result<Checkpoint, Error> {
        if self.documents.contains_key(&document) {
            return Err(Error::DocumentIsActive(document));
        }
        let historical = latest_document(self.root(), document, &commits_from_head(self.root())?)?
            .ok_or(Error::DocumentNotRecoverable(document))?;
        let volume = Volume::from_timestamp(historical.metadata().created())
            .ok_or(Error::InvalidVolumeDate(historical.metadata().created()))?;
        let destination = document_destination(self.root(), volume, document);
        let original_head = head(self.root())?;
        checkpoint_pending_state(self.root(), "Saved state immediately before Restore")?;
        let operation = write_restored_document(&destination, &historical).and_then(|()| {
            require_checkpoint(
                self.root(),
                CheckpointKind::Structural,
                Some("Restored a trashed Document without former Work memberships"),
            )
        });
        match operation {
            Ok(checkpoint) => {
                self.refresh()?;
                Ok(checkpoint)
            }
            Err(error) => {
                rollback_structural(self.root(), &original_head, &[], &[], &[destination], error)?;
                unreachable!("rollback returns the operation error")
            }
        }
    }

    pub fn restore_trashed_work(
        &mut self,
        work: WorkId,
        restore_required_trashed_documents: bool,
    ) -> Result<Checkpoint, Error> {
        if self.works.contains_key(&work) {
            return Err(Error::WorkIsActive(work));
        }
        let commits = commits_from_head(self.root())?;
        let metadata =
            latest_work(self.root(), work, &commits)?.ok_or(Error::WorkNotRecoverable(work))?;
        let key = work_title_key(metadata.title());
        if let Some(existing) = self
            .works
            .values()
            .find(|candidate| work_title_key(candidate.title()) == key)
        {
            return Err(Error::WorkTitleConflict {
                title: metadata.title().to_owned(),
                existing: existing.id(),
            });
        }
        let missing: Vec<_> = metadata
            .documents()
            .iter()
            .copied()
            .filter(|id| !self.documents.contains_key(id))
            .collect();
        let mut restored_documents = Vec::new();
        for id in &missing {
            let document = latest_document(self.root(), *id, &commits)?.ok_or(
                Error::UnrecoverableWorkDocument {
                    work,
                    document: *id,
                    checkpoint: "latest retained history".to_owned(),
                },
            )?;
            restored_documents.push(document);
        }
        if !missing.is_empty() && !restore_required_trashed_documents {
            return Err(Error::TrashedDocumentConsentRequired {
                work,
                documents: missing,
            });
        }

        let original_head = head(self.root())?;
        checkpoint_pending_state(self.root(), "Saved state immediately before Restore")?;
        let work_destination = self.root().join("works").join(work.to_string());
        let mut created = Vec::new();
        let operation = (|| {
            for document in &restored_documents {
                let volume = Volume::from_timestamp(document.metadata().created())
                    .ok_or(Error::InvalidVolumeDate(document.metadata().created()))?;
                let destination =
                    document_destination(self.root(), volume, document.metadata().id());
                write_restored_document(&destination, document)?;
                created.push(destination);
            }
            write_restored_work(&work_destination, &metadata)?;
            created.push(work_destination.clone());
            require_checkpoint(
                self.root(),
                CheckpointKind::Structural,
                Some("Restored a trashed Work"),
            )
        })();
        match operation {
            Ok(checkpoint) => {
                self.refresh()?;
                Ok(checkpoint)
            }
            Err(error) => {
                rollback_structural(self.root(), &original_head, &[], &[], &created, error)?;
                unreachable!("rollback returns the operation error")
            }
        }
    }

    pub fn plan_wipe_document(&self, document: DocumentId) -> Result<WipePlan, Error> {
        if self.documents.contains_key(&document) {
            return Err(Error::DocumentIsActive(document));
        }
        if latest_document(self.root(), document, &commits_from_head(self.root())?)?.is_none() {
            return Err(Error::DocumentNotRecoverable(document));
        }
        require_clean_tracked_state(self.root())?;
        require_single_worktree(self.root())?;
        let refs = retained_refs(self.root())?;
        let commits = commits_from_refs(self.root(), &refs)?;
        let (directories, objects) = wipe_targets(self.root(), document, &commits)?;
        if directories.is_empty() {
            return Err(Error::DocumentNotRecoverable(document));
        }
        verify_objects_are_exclusive(self.root(), document, &commits, &objects)?;
        let planned_head = head(self.root())?;
        let nonce = DocumentId::new_v7();
        let retained_refs = refs
            .iter()
            .map(|reference| reference.name.clone())
            .collect();
        let retained_ref_fingerprint = refs
            .into_iter()
            .map(|reference| (reference.name, reference.object, reference.object_type))
            .collect();
        Ok(WipePlan {
            document,
            planned_head,
            confirmation_token: format!("WIPE {document} {nonce}"),
            canonical_directories: directories.into_iter().collect(),
            content_objects: objects.into_iter().collect(),
            retained_refs,
            retained_ref_fingerprint,
        })
    }

    pub fn execute_wipe_document(
        &mut self,
        plan: &WipePlan,
        confirmation: &str,
    ) -> Result<WipeReport, Error> {
        if confirmation != plan.confirmation_token {
            return Err(Error::WipeConfirmationMismatch);
        }
        if self.documents.contains_key(&plan.document) {
            return Err(Error::DocumentIsActive(plan.document));
        }
        require_clean_tracked_state(self.root())?;
        require_single_worktree(self.root())?;
        if head(self.root())? != plan.planned_head {
            return Err(Error::StaleWipePlan);
        }
        let refs = retained_refs(self.root())?;
        let fingerprint: Vec<_> = refs
            .iter()
            .map(|reference| {
                (
                    reference.name.clone(),
                    reference.object.clone(),
                    reference.object_type.clone(),
                )
            })
            .collect();
        if fingerprint != plan.retained_ref_fingerprint {
            return Err(Error::StaleWipePlan);
        }
        if refs
            .iter()
            .map(|reference| &reference.name)
            .collect::<Vec<_>>()
            != plan.retained_refs.iter().collect::<Vec<_>>()
        {
            return Err(Error::StaleWipePlan);
        }
        let commits = commits_from_refs(self.root(), &refs)?;
        let (directories, objects) = wipe_targets(self.root(), plan.document, &commits)?;
        if directories.iter().cloned().collect::<Vec<_>>() != plan.canonical_directories
            || objects.iter().cloned().collect::<Vec<_>>() != plan.content_objects
        {
            return Err(Error::StaleWipePlan);
        }
        verify_objects_are_exclusive(self.root(), plan.document, &commits, &objects)?;

        let rewrite = rewrite_history(self.root(), plan.document, &refs, &commits)?;
        verify_rewritten_refs(self.root(), plan.document, &objects)?;
        let mut removed_artifacts =
            crate::conflict::scrub_document_conflicts(self.root(), plan.document)?;
        removed_artifacts += remove_carta_artifacts(self.root())?;
        git_output(
            self.root(),
            "expire reflogs after Wipe",
            &[
                "reflog",
                "expire",
                "--expire=now",
                "--expire-unreachable=now",
                "--all",
            ],
        )?;
        git_output(
            self.root(),
            "prune unreachable Wipe objects",
            &["gc", "--prune=now", "--quiet"],
        )?;
        verify_pruned_objects(self.root(), &objects)?;
        self.refresh()?;
        Ok(WipeReport {
            rewritten_refs: rewrite.rewritten_refs,
            rewritten_commits: rewrite.rewritten_commits,
            removed_artifacts,
        })
    }
}

fn checkpoint_pending_state(root: &Path, note: &str) -> Result<(), Error> {
    if crate::history::is_dirty_at(root)? {
        require_checkpoint(root, CheckpointKind::Structural, Some(note))?;
    }
    Ok(())
}

fn head(root: &Path) -> Result<String, Error> {
    let output = git_output(root, "read HEAD", &["rev-parse", "HEAD"])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn commits_from_head(root: &Path) -> Result<Vec<CheckpointId>, Error> {
    let output = git_output(root, "list retained history", &["rev-list", "HEAD"])?;
    parse_commit_ids(&output.stdout)
}

fn parse_commit_ids(bytes: &[u8]) -> Result<Vec<CheckpointId>, Error> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| !line.is_empty())
        .map(CheckpointId::from_str)
        .collect()
}

fn historical_ids(
    root: &Path,
    commits: &[CheckpointId],
) -> Result<(BTreeSet<DocumentId>, BTreeSet<WorkId>), Error> {
    let mut documents = BTreeSet::new();
    let mut works = BTreeSet::new();
    for commit in commits {
        let output = git_output(
            root,
            "scan historical Archive objects",
            &["ls-tree", "-r", "-z", "--name-only", commit.as_str()],
        )?;
        for path in output.stdout.split(|byte| *byte == 0) {
            let Ok(path) = std::str::from_utf8(path) else {
                continue;
            };
            if let Some((id, _)) = canonical_document_path(path) {
                documents.insert(id);
            }
            if let Some(id) = canonical_work_path(path) {
                works.insert(id);
            }
        }
    }
    Ok((documents, works))
}

fn latest_document(
    root: &Path,
    id: DocumentId,
    commits: &[CheckpointId],
) -> Result<Option<Document>, Error> {
    for commit in commits {
        match read_document_at(root, id, commit) {
            Ok(document) => return Ok(Some(document)),
            Err(Error::MissingDocumentRevision { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

fn latest_work(
    root: &Path,
    id: WorkId,
    commits: &[CheckpointId],
) -> Result<Option<WorkMetadata>, Error> {
    let path = format!("works/{id}/work.json");
    for commit in commits {
        let Ok(bytes) = read_blob(root, commit, &path) else {
            continue;
        };
        let metadata = WorkMetadata::read_from(std::io::Cursor::new(bytes))
            .map_err(|error| Error::format(&path, error))?;
        if metadata.id() == id {
            return Ok(Some(metadata));
        }
    }
    Ok(None)
}

fn canonical_document_path(path: &str) -> Option<(DocumentId, String)> {
    let mut parts = path.split('/');
    if parts.next()? != "volumes" {
        return None;
    }
    let year = parts.next()?;
    let month = parts.next()?;
    let id_text = parts.next()?;
    let id = id_text.parse().ok()?;
    if year.len() != 4
        || !year.bytes().all(|byte| byte.is_ascii_digit())
        || month.len() != 2
        || !month.bytes().all(|byte| byte.is_ascii_digit())
        || parts.next().is_none()
    {
        return None;
    }
    Some((id, format!("volumes/{year}/{month}/{id_text}")))
}

fn canonical_work_path(path: &str) -> Option<WorkId> {
    let mut parts = path.split('/');
    if parts.next()? != "works" {
        return None;
    }
    let id = parts.next()?.parse().ok()?;
    (parts.next() == Some("work.json") && parts.next().is_none()).then_some(id)
}

fn document_destination(root: &Path, volume: Volume, document: DocumentId) -> PathBuf {
    root.join("volumes")
        .join(format!("{:04}", volume.year()))
        .join(format!("{:02}", volume.month()))
        .join(document.to_string())
}

fn write_restored_work(destination: &Path, metadata: &WorkMetadata) -> Result<(), Error> {
    let parent = destination.parent().expect("Work destination has a parent");
    fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
    let staging = parent.join(format!(".carta-restore-work-{}", metadata.id()));
    fs::create_dir(&staging).map_err(|error| Error::io(&staging, error))?;
    let path = staging.join("work.json");
    let result = (|| {
        let bytes = crate::archive::serialize_work_metadata(&path, metadata)?;
        fs::write(&path, bytes).map_err(|error| Error::io(&path, error))?;
        fs::rename(&staging, destination).map_err(|error| Error::io(destination, error))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staging);
    }
    result
}

fn transaction_directory(root: &Path, operation: &str, id: String) -> Result<PathBuf, Error> {
    let path = root
        .join(".git")
        .join(format!("carta-transaction-{operation}-{id}"));
    fs::create_dir(&path).map_err(|error| Error::io(&path, error))?;
    Ok(path)
}

fn rollback_structural(
    root: &Path,
    original_head: &str,
    files: &[(PathBuf, Vec<u8>)],
    renames: &[(PathBuf, PathBuf)],
    created: &[PathBuf],
    operation: Error,
) -> Result<(), Error> {
    let rollback = (|| {
        for path in created.iter().rev() {
            if path.exists() {
                fs::remove_dir_all(path).map_err(|error| Error::io(path, error))?;
            }
        }
        for (from, to) in renames.iter().rev() {
            if from.exists() {
                fs::rename(from, to).map_err(|error| Error::io(to, error))?;
            }
        }
        for (path, bytes) in files.iter().rev() {
            crate::archive::atomic_replace(path, bytes)?;
        }
        git_output(
            root,
            "restore failed structural history",
            &["reset", "--mixed", "--quiet", original_head],
        )?;
        Ok(())
    })();
    match rollback {
        Ok(()) => Err(operation),
        Err(rollback) => Err(Error::StructuralRollbackFailed {
            operation: Box::new(operation),
            rollback: Box::new(rollback),
        }),
    }
}

#[derive(Debug, Clone)]
struct RetainedRef {
    name: String,
    object: String,
    object_type: String,
}

fn retained_refs(root: &Path) -> Result<Vec<RetainedRef>, Error> {
    let output = git_output(
        root,
        "list retained refs",
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(objecttype)",
        ],
    )?;
    let mut refs = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split(' ');
        let Some(name) = fields.next() else { continue };
        let Some(object) = fields.next() else {
            continue;
        };
        let Some(object_type) = fields.next() else {
            continue;
        };
        refs.push(RetainedRef {
            name: name.to_owned(),
            object: object.to_owned(),
            object_type: object_type.to_owned(),
        });
    }
    let symbolic_head = git_command(root)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .output()
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })?;
    if !symbolic_head.status.success() {
        let object = head(root)?;
        refs.push(RetainedRef {
            name: "HEAD".to_owned(),
            object,
            object_type: "commit".to_owned(),
        });
    }
    refs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(refs)
}

fn commits_from_refs(root: &Path, refs: &[RetainedRef]) -> Result<Vec<String>, Error> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut command = git_command(root);
    command.args(["rev-list", "--topo-order", "--reverse"]);
    for reference in refs {
        command.arg(&reference.name);
    }
    let output = run_git(command, root, "list commits retained by local refs")?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn wipe_targets(
    root: &Path,
    document: DocumentId,
    commits: &[String],
) -> Result<(BTreeSet<String>, BTreeSet<String>), Error> {
    let mut directories = BTreeSet::new();
    let mut objects = BTreeSet::new();
    for commit in commits {
        for entry in tree_entries(root, commit)? {
            if let Some((id, directory)) = canonical_document_path(&entry.path) {
                if id == document {
                    directories.insert(directory);
                    if entry.object_type == "blob" {
                        objects.insert(entry.object);
                    }
                }
            }
        }
    }
    Ok((directories, objects))
}

fn verify_objects_are_exclusive(
    root: &Path,
    document: DocumentId,
    commits: &[String],
    objects: &BTreeSet<String>,
) -> Result<(), Error> {
    for commit in commits {
        for entry in tree_entries(root, commit)? {
            if objects.contains(&entry.object)
                && !matches!(canonical_document_path(&entry.path), Some((id, _)) if id == document)
            {
                return Err(Error::WipeContentShared {
                    document,
                    object: entry.object,
                    path: entry.path,
                });
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
struct TreeEntry {
    object_type: String,
    object: String,
    path: String,
}

fn tree_entries(root: &Path, object: &str) -> Result<Vec<TreeEntry>, Error> {
    let output = git_output_dynamic(
        root,
        "read retained trees",
        &["ls-tree", "-r", "-z", object],
    )?;
    let mut entries = Vec::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|value| !value.is_empty())
    {
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            return Err(Error::MalformedHistory(
                "ls-tree record lacks a path".to_owned(),
            ));
        };
        let header = String::from_utf8_lossy(&record[..tab]);
        let mut fields = header.split(' ');
        let _mode = fields.next();
        let object_type = fields.next().unwrap_or_default().to_owned();
        let object = fields.next().unwrap_or_default().to_owned();
        let path = std::str::from_utf8(&record[tab + 1..])
            .map_err(|error| Error::MalformedHistory(format!("tree path is not UTF-8: {error}")))?
            .to_owned();
        entries.push(TreeEntry {
            object_type,
            object,
            path,
        });
    }
    Ok(entries)
}

struct RewriteResult {
    rewritten_refs: usize,
    rewritten_commits: usize,
}

fn rewrite_history(
    root: &Path,
    document: DocumentId,
    refs: &[RetainedRef],
    commits: &[String],
) -> Result<RewriteResult, Error> {
    let mut rewritten = HashMap::new();
    let mut changed_commits = 0;
    for commit in commits {
        let raw = git_output_dynamic(
            root,
            "read commit for Wipe",
            &["cat-file", "commit", commit],
        )?
        .stdout;
        let parsed = parse_commit(&raw)?;
        let tree = filter_tree(root, &parsed.tree, document)?;
        let parents: Vec<_> = parsed
            .parents
            .iter()
            .map(|parent| {
                rewritten
                    .get(parent)
                    .cloned()
                    .unwrap_or_else(|| parent.clone())
            })
            .collect();
        if tree == parsed.tree && parents == parsed.parents {
            rewritten.insert(commit.clone(), commit.clone());
            continue;
        }
        let bytes = rewrite_commit_bytes(&raw, &tree, &parents)?;
        let new = hash_object(root, "commit", &bytes)?;
        rewritten.insert(commit.clone(), new);
        changed_commits += 1;
    }

    let mut updates = Vec::new();
    for reference in refs {
        let new = match reference.object_type.as_str() {
            "commit" => rewritten
                .get(&reference.object)
                .cloned()
                .unwrap_or_else(|| reference.object.clone()),
            "tag" => rewrite_tag(root, &reference.object, &rewritten, &mut HashMap::new())?,
            other => {
                return Err(Error::UnsupportedWipeRef {
                    reference: reference.name.clone(),
                    object_type: other.to_owned(),
                })
            }
        };
        if new != reference.object {
            updates.push((reference.name.clone(), new, reference.object.clone()));
        }
    }
    update_refs_transaction(root, &updates)?;
    git_output(
        root,
        "refresh index after Wipe",
        &["reset", "--mixed", "--quiet"],
    )?;
    Ok(RewriteResult {
        rewritten_refs: updates.len(),
        rewritten_commits: changed_commits,
    })
}

struct ParsedCommit {
    tree: String,
    parents: Vec<String>,
}

fn parse_commit(raw: &[u8]) -> Result<ParsedCommit, Error> {
    let separator = raw
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or_else(|| Error::MalformedHistory("commit lacks a message separator".to_owned()))?;
    let headers = std::str::from_utf8(&raw[..separator]).map_err(|error| {
        Error::MalformedHistory(format!("commit headers are not UTF-8: {error}"))
    })?;
    let mut tree = None;
    let mut parents = Vec::new();
    for line in headers.lines() {
        if let Some(value) = line.strip_prefix("tree ") {
            tree = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix("parent ") {
            parents.push(value.to_owned());
        }
    }
    Ok(ParsedCommit {
        tree: tree.ok_or_else(|| Error::MalformedHistory("commit lacks a tree".to_owned()))?,
        parents,
    })
}

fn rewrite_commit_bytes(raw: &[u8], tree: &str, parents: &[String]) -> Result<Vec<u8>, Error> {
    let separator = raw
        .windows(2)
        .position(|window| window == b"\n\n")
        .ok_or_else(|| Error::MalformedHistory("commit lacks a message separator".to_owned()))?;
    let headers = std::str::from_utf8(&raw[..separator]).map_err(|error| {
        Error::MalformedHistory(format!("commit headers are not UTF-8: {error}"))
    })?;
    let mut output = Vec::new();
    writeln!(output, "tree {tree}").expect("Vec writes cannot fail");
    for parent in parents {
        writeln!(output, "parent {parent}").expect("Vec writes cannot fail");
    }
    let mut skipping_signature = false;
    for line in headers.lines() {
        if line.starts_with("tree ") || line.starts_with("parent ") {
            continue;
        }
        if line.starts_with("gpgsig ") || line.starts_with("gpgsig-sha256 ") {
            skipping_signature = true;
            continue;
        }
        if skipping_signature && line.starts_with(' ') {
            continue;
        }
        skipping_signature = false;
        writeln!(output, "{line}").expect("Vec writes cannot fail");
    }
    output.push(b'\n');
    output.extend_from_slice(&raw[separator + 2..]);
    Ok(output)
}

fn filter_tree(root: &Path, tree: &str, document: DocumentId) -> Result<String, Error> {
    let paths: Vec<_> = tree_entries(root, tree)?
        .into_iter()
        .filter(
            |entry| matches!(canonical_document_path(&entry.path), Some((id, _)) if id == document),
        )
        .map(|entry| entry.path)
        .collect();
    if paths.is_empty() {
        return Ok(tree.to_owned());
    }
    let index = root
        .join(".git")
        .join(format!("carta-wipe-index-{}", DocumentId::new_v7()));
    let result = (|| {
        git_with_index(
            root,
            &index,
            "load tree for Wipe",
            &["read-tree", tree],
            &[],
        )?;
        let mut input = Vec::new();
        for path in &paths {
            input.extend_from_slice(path.as_bytes());
            input.push(0);
        }
        git_with_index(
            root,
            &index,
            "remove Document paths from tree",
            &["update-index", "--force-remove", "-z", "--stdin"],
            &input,
        )?;
        let output = git_with_index(root, &index, "write filtered tree", &["write-tree"], &[])?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    })();
    let _ = fs::remove_file(&index);
    let _ = fs::remove_file(index.with_extension("lock"));
    result
}

fn git_with_index(
    root: &Path,
    index: &Path,
    operation: &'static str,
    args: &[&str],
    input: &[u8],
) -> Result<Output, Error> {
    let mut command = git_command(root);
    command
        .env("GIT_INDEX_FILE", index)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run_git_with_input(command, root, operation, input)
}

fn hash_object(root: &Path, object_type: &str, bytes: &[u8]) -> Result<String, Error> {
    let mut command = git_command(root);
    command
        .args(["hash-object", "-w", "-t", object_type, "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = run_git_with_input(command, root, "write rewritten Git object", bytes)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn rewrite_tag(
    root: &Path,
    tag: &str,
    commits: &HashMap<String, String>,
    tags: &mut HashMap<String, String>,
) -> Result<String, Error> {
    if let Some(rewritten) = tags.get(tag) {
        return Ok(rewritten.clone());
    }
    let raw = git_output_dynamic(
        root,
        "read annotated tag for Wipe",
        &["cat-file", "tag", tag],
    )?
    .stdout;
    let separator = raw
        .windows(2)
        .position(|window| window == b"\n\n")
        .unwrap_or(raw.len());
    let headers = std::str::from_utf8(&raw[..separator])
        .map_err(|error| Error::MalformedHistory(format!("tag headers are not UTF-8: {error}")))?;
    let object = headers
        .lines()
        .find_map(|line| line.strip_prefix("object "))
        .ok_or_else(|| Error::MalformedHistory("tag lacks an object".to_owned()))?;
    let object_type = headers
        .lines()
        .find_map(|line| line.strip_prefix("type "))
        .unwrap_or_default();
    let rewritten_object = match object_type {
        "commit" => commits
            .get(object)
            .cloned()
            .unwrap_or_else(|| object.to_owned()),
        "tag" => rewrite_tag(root, object, commits, tags)?,
        other => {
            return Err(Error::UnsupportedWipeRef {
                reference: tag.to_owned(),
                object_type: other.to_owned(),
            })
        }
    };
    if rewritten_object == object {
        tags.insert(tag.to_owned(), tag.to_owned());
        return Ok(tag.to_owned());
    }
    let mut rewritten = raw.clone();
    let old = format!("object {object}");
    let new = format!("object {rewritten_object}");
    rewritten.splice(0..old.len(), new.bytes());
    let new_tag = hash_object(root, "tag", &rewritten)?;
    tags.insert(tag.to_owned(), new_tag.clone());
    Ok(new_tag)
}

fn update_refs_transaction(root: &Path, updates: &[(String, String, String)]) -> Result<(), Error> {
    if updates.is_empty() {
        return Ok(());
    }
    let mut input = String::from("start\n");
    for (name, new, old) in updates {
        input.push_str(&format!("update {name} {new} {old}\n"));
    }
    input.push_str("prepare\ncommit\n");
    let mut command = git_command(root);
    command
        .args(["update-ref", "--stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run_git_with_input(
        command,
        root,
        "atomically update rewritten refs",
        input.as_bytes(),
    )?;
    Ok(())
}

fn verify_rewritten_refs(
    root: &Path,
    document: DocumentId,
    content_objects: &BTreeSet<String>,
) -> Result<(), Error> {
    let refs = retained_refs(root)?;
    let commits = commits_from_refs(root, &refs)?;
    let (directories, _) = wipe_targets(root, document, &commits)?;
    if !directories.is_empty() {
        return Err(Error::WipeVerificationFailed(format!(
            "Document {document} still has canonical paths in retained refs"
        )));
    }
    let reachable = reachable_objects(root)?;
    if let Some(object) = content_objects
        .iter()
        .find(|object| reachable.contains(*object))
    {
        return Err(Error::WipeVerificationFailed(format!(
            "Document content object {object} remains reachable"
        )));
    }
    Ok(())
}

fn reachable_objects(root: &Path) -> Result<HashSet<String>, Error> {
    let output = git_output(
        root,
        "enumerate reachable objects",
        &["rev-list", "--objects", "--all"],
    )?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect())
}

fn verify_pruned_objects(root: &Path, objects: &BTreeSet<String>) -> Result<(), Error> {
    for object in objects {
        let status = git_command(root)
            .args(["cat-file", "-e", object])
            .status()
            .map_err(|source| Error::GitUnavailable {
                path: root.to_path_buf(),
                source,
            })?;
        if status.success() {
            return Err(Error::WipeVerificationFailed(format!(
                "content object {object} remains in the Git object database after pruning"
            )));
        }
    }
    git_output(
        root,
        "verify Git object database after Wipe",
        &["fsck", "--full", "--no-reflogs"],
    )?;
    Ok(())
}

fn require_clean_tracked_state(root: &Path) -> Result<(), Error> {
    let output = git_output(
        root,
        "inspect tracked state before Wipe",
        &["status", "--porcelain=v1", "-z", "--untracked-files=no"],
    )?;
    if output.stdout.is_empty() {
        Ok(())
    } else {
        Err(Error::StructuralOperationRequiresCleanArchive)
    }
}

fn require_single_worktree(root: &Path) -> Result<(), Error> {
    let output = git_output(
        root,
        "inspect Archive worktrees before Wipe",
        &["worktree", "list", "--porcelain"],
    )?;
    let count = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.starts_with("worktree "))
        .count();
    if count == 1 {
        Ok(())
    } else {
        Err(Error::WipeAdditionalWorktrees)
    }
}

fn remove_carta_artifacts(root: &Path) -> Result<usize, Error> {
    let mut removed = 0;
    remove_reserved_children(root, false, &mut removed)?;
    remove_reserved_children(&root.join(".git"), true, &mut removed)?;
    Ok(removed)
}

fn remove_reserved_children(
    path: &Path,
    git_directory: bool,
    removed: &mut usize,
) -> Result<(), Error> {
    if !path.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(|error| Error::io(path, error))? {
        let entry = entry.map_err(|error| Error::io(path, error))?;
        let child = entry.path();
        if !git_directory && child.file_name().is_some_and(|name| name == ".git") {
            continue;
        }
        let reserved = child
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                if git_directory {
                    name.starts_with("carta-") && name != "carta-conflicts"
                } else {
                    name.starts_with(".carta-")
                }
            });
        if reserved {
            if child.is_dir() {
                fs::remove_dir_all(&child).map_err(|error| Error::io(&child, error))?;
            } else {
                fs::remove_file(&child).map_err(|error| Error::io(&child, error))?;
            }
            *removed += 1;
        } else if child.is_dir() && !git_directory {
            remove_reserved_children(&child, false, removed)?;
        }
    }
    Ok(())
}

fn git_output_dynamic(
    root: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<Output, Error> {
    let mut command = git_command(root);
    command.args(args);
    run_git(command, root, operation)
}

fn run_git(mut command: Command, root: &Path, operation: &'static str) -> Result<Output, Error> {
    let output = command.output().map_err(|source| Error::GitUnavailable {
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

fn run_git_with_input(
    mut command: Command,
    root: &Path,
    operation: &'static str,
    input: &[u8],
) -> Result<Output, Error> {
    let mut child = command.spawn().map_err(|source| Error::GitUnavailable {
        path: root.to_path_buf(),
        source,
    })?;
    child
        .stdin
        .take()
        .expect("Git stdin was piped")
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
