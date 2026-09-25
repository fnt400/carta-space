use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::history::{git_output, require_checkpoint};
use crate::{CheckpointKind, DocumentId, Error, WorkId};

const MANIFEST: &str = "carta-transaction.json";

#[derive(Debug, Serialize, Deserialize)]
struct TransactionManifest {
    checkpoint: String,
    owned: Vec<OwnedPath>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OwnedPath {
    path: PathBuf,
    state: PathState,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum PathState {
    Missing,
    File { bytes: Vec<u8> },
    Directory { entries: Vec<SnapshotEntry> },
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotEntry {
    path: PathBuf,
    directory: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bytes: Vec<u8>,
}

struct RecoveredDocumentParts {
    document: DocumentId,
    directory: PathBuf,
    previous_content: Vec<u8>,
    metadata: Vec<u8>,
}

pub(crate) struct Transaction {
    root: PathBuf,
}

impl Transaction {
    pub(crate) fn begin(
        root: &Path,
        note: &str,
        owned: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, Error> {
        recover(root)?;
        if crate::history::is_dirty_at(root)? {
            require_checkpoint(root, CheckpointKind::Structural, Some(note))?;
        }
        let checkpoint = String::from_utf8_lossy(
            &git_output(root, "read transaction checkpoint", &["rev-parse", "HEAD"])?.stdout,
        )
        .trim()
        .to_owned();
        let mut paths: Vec<_> = owned.into_iter().collect();
        paths.sort();
        paths.dedup();
        for path in &paths {
            validate_relative(path)?;
        }
        reject_overlapping_paths(&paths)?;
        let owned = paths
            .into_iter()
            .map(|path| {
                let state = snapshot_path(&root.join(&path))?;
                Ok(OwnedPath { path, state })
            })
            .collect::<Result<_, Error>>()?;
        write_manifest(root, &TransactionManifest { checkpoint, owned })?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub(crate) fn commit(self) -> Result<(), Error> {
        remove_manifest(&self.root)
    }

    pub(crate) fn rollback(self) -> Result<(), Error> {
        recover_internal(&self.root, false)
    }

    pub(crate) fn rollback_error(self, operation: Error) -> Error {
        match self.rollback() {
            Ok(()) => operation,
            Err(rollback) => Error::StructuralRollbackFailed {
                operation: Box::new(operation),
                rollback: Box::new(rollback),
            },
        }
    }
}

pub(crate) fn recover(root: &Path) -> Result<(), Error> {
    recover_internal(root, true)
}

fn recover_internal(root: &Path, preserve_ambiguity: bool) -> Result<(), Error> {
    let path = manifest_path(root);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::io(&path, error)),
    };
    let manifest: TransactionManifest = serde_json::from_slice(&bytes)
        .map_err(|error| Error::MalformedTransaction(error.to_string()))?;
    let paths: Vec<_> = manifest
        .owned
        .iter()
        .map(|owned| owned.path.clone())
        .collect();
    for relative in &paths {
        validate_relative(relative)?;
    }
    reject_overlapping_paths(&paths)?;
    reject_unowned_tracked_changes(root, &manifest.checkpoint, &paths)?;

    if preserve_ambiguity {
        // A conflict-resolution transaction owns its existing conflict record.
        // Restore that record first so canonical-path ambiguity reuses it
        // instead of creating a duplicate recovery choice.
        for owned in manifest
            .owned
            .iter()
            .filter(|owned| owned.path.starts_with(".git/carta-conflicts"))
        {
            restore_path(root, owned)?;
        }
        for owned in &manifest.owned {
            if owned.path.starts_with(".git/carta-conflicts") {
                continue;
            }
            let current = snapshot_path(&root.join(&owned.path))?;
            if current != owned.state {
                preserve_owned_ambiguity(root, owned, &current)?;
            }
        }
    }

    let current = String::from_utf8_lossy(
        &git_output(
            root,
            "read interrupted transaction HEAD",
            &["rev-parse", "HEAD"],
        )?
        .stdout,
    )
    .trim()
    .to_owned();
    if current != manifest.checkpoint {
        git_output(
            root,
            "rewind interrupted structural operation",
            &["update-ref", "HEAD", &manifest.checkpoint, &current],
        )?;
    }
    for owned in &manifest.owned {
        restore_path(root, owned)?;
    }
    let tracked: Vec<_> = paths
        .iter()
        .filter(|path| !path.starts_with(".git"))
        .filter_map(|path| path.to_str())
        .collect();
    if !tracked.is_empty() {
        let mut args = vec!["reset", "--quiet", &manifest.checkpoint, "--"];
        args.extend(tracked);
        git_output(root, "restore transaction-owned index paths", &args)?;
    }
    remove_manifest(root)
}

fn preserve_owned_ambiguity(
    root: &Path,
    owned: &OwnedPath,
    current: &PathState,
) -> Result<(), Error> {
    if let Some(parts) = document_conflict_parts(root, owned, current)? {
        let current_content = current_document_content(root, owned, current)?;
        crate::conflict::preserve_recovered_document_conflict(
            root,
            parts.document,
            &current_content,
            &parts.previous_content,
            parts.directory,
            parts.metadata,
        )?;
        return Ok(());
    }

    if let Some((work, previous)) = work_conflict_parts(owned) {
        let current_bytes = current_file_bytes(current).unwrap_or_default();
        crate::conflict::preserve_recovered_work_conflict(
            root,
            work,
            current_bytes,
            previous,
            owned.path.clone(),
        )?;
    }
    Ok(())
}

fn document_conflict_parts(
    root: &Path,
    owned: &OwnedPath,
    current: &PathState,
) -> Result<Option<RecoveredDocumentParts>, Error> {
    let (directory, file_name) = if owned
        .path
        .file_name()
        .is_some_and(|name| name == "content.md")
    {
        (
            owned.path.parent().unwrap().to_path_buf(),
            Some("content.md"),
        )
    } else {
        (owned.path.clone(), None)
    };
    let Some(id) = directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.parse::<DocumentId>().ok())
    else {
        return Ok(None);
    };
    if !directory.starts_with("volumes") {
        return Ok(None);
    }

    let previous_content = if file_name.is_some() {
        current_file_bytes(&owned.state)
            .unwrap_or_default()
            .to_vec()
    } else {
        snapshot_entry_bytes(&owned.state, Path::new("content.md")).unwrap_or_default()
    };
    let metadata = if file_name.is_some() {
        let path = root.join(&directory).join("meta.json");
        fs::read(&path).map_err(|error| Error::io(path, error))?
    } else {
        snapshot_entry_bytes(&owned.state, Path::new("meta.json")).unwrap_or_default()
    };
    if previous_content.is_empty() && matches!(owned.state, PathState::Missing) {
        return Ok(None);
    }
    let _ = current;
    Ok(Some(RecoveredDocumentParts {
        document: id,
        directory,
        previous_content,
        metadata,
    }))
}

fn current_document_content(
    root: &Path,
    owned: &OwnedPath,
    current: &PathState,
) -> Result<Vec<u8>, Error> {
    if owned
        .path
        .file_name()
        .is_some_and(|name| name == "content.md")
    {
        return Ok(current_file_bytes(current).unwrap_or_default().to_vec());
    }
    if let Some(bytes) = snapshot_entry_bytes(current, Path::new("content.md")) {
        return Ok(bytes);
    }
    let path = root.join(&owned.path).join("content.md");
    match fs::read(&path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(Error::io(path, error)),
    }
}

fn work_conflict_parts(owned: &OwnedPath) -> Option<(WorkId, &[u8])> {
    let path = if owned
        .path
        .file_name()
        .is_some_and(|name| name == "work.json")
    {
        &owned.path
    } else {
        return None;
    };
    let work = path.parent()?.file_name()?.to_str()?.parse().ok()?;
    path.parent()?
        .parent()?
        .file_name()
        .is_some_and(|name| name == "works")
        .then_some((work, current_file_bytes(&owned.state).unwrap_or_default()))
}

fn current_file_bytes(state: &PathState) -> Option<&[u8]> {
    match state {
        PathState::File { bytes } => Some(bytes),
        _ => None,
    }
}

fn snapshot_entry_bytes(state: &PathState, wanted: &Path) -> Option<Vec<u8>> {
    let PathState::Directory { entries } = state else {
        return None;
    };
    entries
        .iter()
        .find(|entry| !entry.directory && entry.path == wanted)
        .map(|entry| entry.bytes.clone())
}

fn reject_unowned_tracked_changes(
    root: &Path,
    checkpoint: &str,
    owned: &[PathBuf],
) -> Result<(), Error> {
    let output = git_output(
        root,
        "inspect interrupted transaction changes",
        &["diff", "--name-only", "-z", checkpoint],
    )?;
    let changed = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| PathBuf::from(String::from_utf8_lossy(path).into_owned()))
        .collect::<Vec<_>>();
    let ambiguous: Vec<_> = changed
        .into_iter()
        .filter(|changed| !owned.iter().any(|path| changed.starts_with(path)))
        .collect();
    if ambiguous.is_empty() {
        Ok(())
    } else {
        Err(Error::AmbiguousTransactionRecovery(ambiguous))
    }
}

fn snapshot_path(path: &Path) -> Result<PathState, Error> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PathState::Missing)
        }
        Err(error) => return Err(Error::io(path, error)),
    };
    if metadata.is_file() {
        return Ok(PathState::File {
            bytes: fs::read(path).map_err(|error| Error::io(path, error))?,
        });
    }
    if !metadata.is_dir() {
        return Err(Error::UnsafeTransactionPath(path.to_path_buf()));
    }
    let mut entries = Vec::new();
    snapshot_directory(path, path, &mut entries)?;
    Ok(PathState::Directory { entries })
}

fn snapshot_directory(
    root: &Path,
    path: &Path,
    entries: &mut Vec<SnapshotEntry>,
) -> Result<(), Error> {
    let mut children = fs::read_dir(path)
        .map_err(|error| Error::io(path, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| Error::io(path, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    children.sort();
    for child in children {
        let metadata = fs::symlink_metadata(&child).map_err(|error| Error::io(&child, error))?;
        let relative = child
            .strip_prefix(root)
            .expect("snapshot child is under root")
            .to_path_buf();
        if metadata.is_dir() {
            entries.push(SnapshotEntry {
                path: relative,
                directory: true,
                bytes: Vec::new(),
            });
            snapshot_directory(root, &child, entries)?;
        } else if metadata.is_file() {
            entries.push(SnapshotEntry {
                path: relative,
                directory: false,
                bytes: fs::read(&child).map_err(|error| Error::io(&child, error))?,
            });
        } else {
            return Err(Error::UnsafeTransactionPath(child));
        }
    }
    Ok(())
}

fn restore_path(root: &Path, owned: &OwnedPath) -> Result<(), Error> {
    let destination = root.join(&owned.path);
    remove_path(&destination)?;
    match &owned.state {
        PathState::Missing => Ok(()),
        PathState::File { bytes } => {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
            }
            crate::archive::atomic_replace(&destination, bytes)
        }
        PathState::Directory { entries } => {
            fs::create_dir_all(&destination).map_err(|error| Error::io(&destination, error))?;
            for entry in entries.iter().filter(|entry| entry.directory) {
                let path = destination.join(&entry.path);
                fs::create_dir_all(&path).map_err(|error| Error::io(&path, error))?;
            }
            for entry in entries.iter().filter(|entry| !entry.directory) {
                let path = destination.join(&entry.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| Error::io(parent, error))?;
                }
                crate::archive::atomic_replace(&path, &entry.bytes)?;
            }
            Ok(())
        }
    }
}

fn remove_path(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path).map_err(|error| Error::io(path, error))
        }
        Ok(_) => fs::remove_file(path).map_err(|error| Error::io(path, error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io(path, error)),
    }
}

fn reject_overlapping_paths(paths: &[PathBuf]) -> Result<(), Error> {
    for (index, path) in paths.iter().enumerate() {
        if paths
            .iter()
            .skip(index + 1)
            .any(|other| other.starts_with(path))
        {
            return Err(Error::UnsafeTransactionPath(path.clone()));
        }
    }
    Ok(())
}

fn write_manifest(root: &Path, manifest: &TransactionManifest) -> Result<(), Error> {
    let git = root.join(".git");
    let temporary = git.join(format!(".carta-transaction-{}.tmp", DocumentId::new_v7()));
    let destination = manifest_path(root);
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| Error::MalformedTransaction(error.to_string()))?;
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
        fs::rename(&temporary, &destination).map_err(|error| Error::io(&destination, error))?;
        File::open(&git)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| Error::io(&git, error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn remove_manifest(root: &Path) -> Result<(), Error> {
    let path = manifest_path(root);
    match fs::remove_file(&path) {
        Ok(()) => File::open(root.join(".git"))
            .and_then(|directory| directory.sync_all())
            .map_err(|error| Error::io(root.join(".git"), error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(Error::io(path, error)),
    }
}

fn manifest_path(root: &Path) -> PathBuf {
    root.join(".git").join(MANIFEST)
}

fn validate_relative(path: &Path) -> Result<(), Error> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(Error::UnsafeTransactionPath(path.to_path_buf()));
    }
    Ok(())
}
