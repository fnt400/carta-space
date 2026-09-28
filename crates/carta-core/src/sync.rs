use std::fs;
use std::io::Cursor;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use carta_format::{ArchiveId, ArchiveMetadata};

use crate::{Archive, Error};

const SYNC_REMOTE: &str = "carta-sync";
const SYNC_TRACKING_REF: &str = "refs/remotes/carta-sync/carta";
const CARTA_AUTHOR_NAME: &str = "Carta Space";
const CARTA_AUTHOR_EMAIL: &str = "history@carta.space";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncOutcome {
    Disabled,
    Synced,
    Published,
    UpdatedFromRemote,
    Merged,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    outcome: SyncOutcome,
}

impl SyncReport {
    fn new(outcome: SyncOutcome) -> Self {
        Self { outcome }
    }

    pub fn outcome(&self) -> SyncOutcome {
        self.outcome
    }
}

impl Archive {
    pub fn clone_sync_remote(
        url: &str,
        destination: impl AsRef<Path>,
    ) -> Result<Self, Error> {
        let destination = destination.as_ref();
        match fs::symlink_metadata(destination) {
            Ok(_) => return Err(Error::AlreadyExists(destination.to_path_buf())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(destination, error)),
        }

        let url = url.trim();
        if url.is_empty() {
            return Err(Error::InvalidSyncRemote(
                "remote repository URL is empty".to_owned(),
            ));
        }

        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let staging = parent.join(format!(".carta-clone-{}", ArchiveId::new_v7()));

        let status = Command::new("git")
            .args([
                "clone",
                "--branch",
                "carta",
                "--single-branch",
                "--origin",
                SYNC_REMOTE,
                "--",
            ])
            .arg(url)
            .arg(&staging)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|source| Error::GitUnavailable {
                path: destination.to_path_buf(),
                source,
            })?;
        if !status.success() {
            let _ = fs::remove_dir_all(&staging);
            return Err(Error::GitCloneFailed { status });
        }

        let result = (|| {
            {
                let archive = Self::open(&staging)?;
                archive.set_sync_remote(url)?;
            }

            fs::rename(&staging, destination).map_err(|error| Error::io(destination, error))?;
            match Self::open(destination) {
                Ok(archive) => Ok(archive),
                Err(error) => {
                    let _ = fs::rename(destination, &staging);
                    Err(error)
                }
            }
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }

    pub fn sync_remote(&self) -> Result<Option<String>, Error> {
        optional_git_text(
            &self.root,
            "read sync remote",
            &["config", "--get", "remote.carta-sync.url"],
        )
    }

    pub fn set_sync_remote(&self, url: &str) -> Result<(), Error> {
        let url = url.trim();
        if url.is_empty() {
            return self.clear_sync_remote().map(|_| ());
        }
        crate::history::git_output(
            &self.root,
            "configure sync remote URL",
            &["config", "remote.carta-sync.url", url],
        )?;
        crate::history::git_output(
            &self.root,
            "configure sync remote fetch",
            &[
                "config",
                "--replace-all",
                "remote.carta-sync.fetch",
                "+refs/heads/*:refs/remotes/carta-sync/*",
            ],
        )?;
        Ok(())
    }

    pub fn clear_sync_remote(&self) -> Result<bool, Error> {
        if self.sync_remote()?.is_none() {
            return Ok(false);
        }
        crate::history::git_output(
            &self.root,
            "remove sync remote",
            &["config", "--remove-section", "remote.carta-sync"],
        )?;
        let _ = crate::history::git_output(
            &self.root,
            "remove sync tracking reference",
            &["update-ref", "-d", SYNC_TRACKING_REF],
        );
        Ok(true)
    }

    pub fn sync(&mut self) -> Result<SyncReport, Error> {
        if self.sync_remote()?.is_none() {
            return Ok(SyncReport::new(SyncOutcome::Disabled));
        }
        if crate::history::is_dirty_at(&self.root)? {
            return Err(Error::SyncRequiresCleanArchive);
        }

        match remote_state(&self.root)? {
            RemoteState::Empty => {
                push_head(&self.root)?;
                Ok(SyncReport::new(SyncOutcome::Published))
            }
            RemoteState::OtherBranches => Err(Error::SyncRemoteNotEmpty),
            RemoteState::Carta => {
                fetch_carta(&self.root)?;
                validate_remote_archive(self)?;

                let local = current_head(&self.root)?;
                let remote = current_tracking_head(&self.root)?;
                if local == remote {
                    return Ok(SyncReport::new(SyncOutcome::Synced));
                }

                if is_ancestor(&self.root, &remote, &local)? {
                    push_head(&self.root)?;
                    return Ok(SyncReport::new(SyncOutcome::Published));
                }

                if is_ancestor(&self.root, &local, &remote)? {
                    reset_to(&self.root, &remote)?;
                    self.refresh()?;
                    return Ok(SyncReport::new(SyncOutcome::UpdatedFromRemote));
                }

                match merged_tree(&self.root, &local, &remote)? {
                    MergeTree::Conflict => Ok(SyncReport::new(SyncOutcome::Conflict)),
                    MergeTree::Clean(tree) => {
                        let merge = create_merge_commit(&self.root, &tree, &local, &remote)?;
                        reset_to(&self.root, &merge)?;
                        self.refresh()?;
                        push_head(&self.root)?;
                        Ok(SyncReport::new(SyncOutcome::Merged))
                    }
                }
            }
        }
    }
}

enum RemoteState {
    Empty,
    OtherBranches,
    Carta,
}

enum MergeTree {
    Clean(String),
    Conflict,
}

fn remote_state(root: &Path) -> Result<RemoteState, Error> {
    let output = crate::history::git_output(
        root,
        "inspect sync remote",
        &["ls-remote", "--heads", SYNC_REMOTE],
    )?;
    let text = text_output(output, "inspect sync remote")?;
    let mut any = false;
    for line in text.lines() {
        any = true;
        if line.split_whitespace().nth(1) == Some("refs/heads/carta") {
            return Ok(RemoteState::Carta);
        }
    }
    if any {
        Ok(RemoteState::OtherBranches)
    } else {
        Ok(RemoteState::Empty)
    }
}

fn fetch_carta(root: &Path) -> Result<(), Error> {
    crate::history::git_output(
        root,
        "fetch sync remote",
        &[
            "fetch",
            "--quiet",
            "--no-tags",
            SYNC_REMOTE,
            "+refs/heads/carta:refs/remotes/carta-sync/carta",
        ],
    )?;
    Ok(())
}

fn validate_remote_archive(archive: &Archive) -> Result<(), Error> {
    let object = format!("{SYNC_TRACKING_REF}:carta.json");
    let output = crate::history::git_output(
        &archive.root,
        "read sync remote Archive metadata",
        &["show", &object],
    )?;
    let remote = ArchiveMetadata::read_from(Cursor::new(output.stdout))
        .map_err(|error| Error::InvalidSyncRemote(error.to_string()))?;
    let local_id = archive.metadata().archive_id();
    let remote_id = remote.archive_id();
    if local_id != remote_id {
        return Err(Error::SyncArchiveMismatch {
            local: local_id.to_string(),
            remote: remote_id.to_string(),
        });
    }
    Ok(())
}

fn current_head(root: &Path) -> Result<String, Error> {
    required_git_text(
        root,
        "read local sync head",
        &["rev-parse", "--verify", "HEAD"],
    )
}

fn current_tracking_head(root: &Path) -> Result<String, Error> {
    required_git_text(
        root,
        "read fetched sync head",
        &["rev-parse", "--verify", SYNC_TRACKING_REF],
    )
}

fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Result<bool, Error> {
    let output = raw_git_output(root, &["merge-base", "--is-ancestor", ancestor, descendant])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(command_failed("compare sync history", output)),
    }
}

fn merged_tree(root: &Path, local: &str, remote: &str) -> Result<MergeTree, Error> {
    let output = raw_git_output(root, &["merge-tree", "--write-tree", local, remote])?;
    match output.status.code() {
        Some(0) => {
            let text = String::from_utf8(output.stdout)
                .map_err(|error| Error::InvalidSyncRemote(error.to_string()))?;
            let tree = text.lines().next().unwrap_or_default().trim();
            if tree.is_empty() {
                return Err(Error::InvalidSyncRemote(
                    "Git merge-tree produced no tree identifier".into(),
                ));
            }
            Ok(MergeTree::Clean(tree.to_owned()))
        }
        Some(1) => Ok(MergeTree::Conflict),
        _ => Err(command_failed("prepare sync merge", output)),
    }
}

fn create_merge_commit(
    root: &Path,
    tree: &str,
    local: &str,
    remote: &str,
) -> Result<String, Error> {
    let author_name = format!("user.name={CARTA_AUTHOR_NAME}");
    let author_email = format!("user.email={CARTA_AUTHOR_EMAIL}");
    let output = crate::history::git_with_input(
        root,
        "create sync merge",
        &[
            "-c",
            &author_name,
            "-c",
            &author_email,
            "commit-tree",
            tree,
            "-p",
            local,
            "-p",
            remote,
        ],
        b"Carta sync merge\n",
    )?;
    let merge = String::from_utf8(output.stdout)
        .map_err(|error| Error::InvalidSyncRemote(error.to_string()))?;
    let merge = merge.trim();
    if merge.is_empty() {
        return Err(Error::InvalidSyncRemote(
            "Git commit-tree produced no commit identifier".into(),
        ));
    }
    Ok(merge.to_owned())
}

fn reset_to(root: &Path, commit: &str) -> Result<(), Error> {
    crate::history::git_output(
        root,
        "update working tree from sync",
        &["reset", "--hard", "--quiet", commit],
    )?;
    Ok(())
}

fn push_head(root: &Path) -> Result<(), Error> {
    crate::history::git_output(
        root,
        "push sync remote",
        &["push", "--quiet", SYNC_REMOTE, "HEAD:refs/heads/carta"],
    )?;
    Ok(())
}

fn required_git_text(root: &Path, operation: &'static str, args: &[&str]) -> Result<String, Error> {
    let output = crate::history::git_output(root, operation, args)?;
    let text = text_output(output, operation)?;
    let value = text.trim();
    if value.is_empty() {
        Err(Error::InvalidSyncRemote(format!(
            "Git produced no output while attempting to {operation}"
        )))
    } else {
        Ok(value.to_owned())
    }
}

fn optional_git_text(
    root: &Path,
    operation: &'static str,
    args: &[&str],
) -> Result<Option<String>, Error> {
    let output = raw_git_output(root, args)?;
    match output.status.code() {
        Some(0) => {
            let text = text_output(output, operation)?;
            let value = text.trim();
            Ok((!value.is_empty()).then(|| value.to_owned()))
        }
        Some(1) if output.stdout.is_empty() => Ok(None),
        _ => Err(command_failed(operation, output)),
    }
}

fn text_output(output: Output, operation: &'static str) -> Result<String, Error> {
    String::from_utf8(output.stdout)
        .map_err(|error| Error::InvalidSyncRemote(format!("{operation}: {error}")))
}

fn raw_git_output(root: &Path, args: &[&str]) -> Result<Output, Error> {
    crate::history::git_command(root)
        .args(args)
        .output()
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })
}

fn command_failed(operation: &'static str, output: Output) -> Error {
    Error::GitCommandFailed {
        operation,
        status: output.status,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::SyncOutcome;
    use crate::{Archive, CheckpointKind, Error};

    fn run_git(directory: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(directory)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed with {status}");
    }

    fn bare_remote(root: &Path) -> PathBuf {
        let remote = root.join("remote.git");
        let status = Command::new("git")
            .args(["init", "--bare", "--quiet"])
            .arg(&remote)
            .status()
            .unwrap();
        assert!(status.success());
        remote
    }

    fn clone_archive(remote: &Path, destination: &Path) -> Archive {
        let status = Command::new("git")
            .args(["clone", "--quiet", "--branch", "carta"])
            .arg(remote)
            .arg(destination)
            .status()
            .unwrap();
        assert!(status.success());
        let archive = Archive::open(destination).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        archive
    }

    #[test]
    fn clone_sync_remote_uses_carta_branch_even_when_remote_head_is_unborn() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());

        let mut source = Archive::create(temporary.path().join("source")).unwrap();
        let document = source.create_document("remote document").unwrap();
        source
            .checkpoint(CheckpointKind::Structural, Some("remote document"))
            .unwrap();
        source.set_sync_remote(remote.to_str().unwrap()).unwrap();
        source.sync().unwrap();

        let imported_path = temporary.path().join("imported");
        let imported =
            Archive::clone_sync_remote(remote.to_str().unwrap(), &imported_path).unwrap();

        assert_eq!(
            imported.read_document(document).unwrap().content(),
            "remote document"
        );
        assert_eq!(imported.sync_remote().unwrap().as_deref(), remote.to_str());
        assert!(imported_path.join("carta.json").is_file());

        let branch = Command::new("git")
            .current_dir(&imported_path)
            .args(["branch", "--show-current"])
            .output()
            .unwrap();
        assert!(branch.status.success());
        assert_eq!(String::from_utf8_lossy(&branch.stdout).trim(), "carta");
    }

    #[test]
    fn failed_clone_sync_remote_leaves_destination_absent() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let destination = temporary.path().join("imported");

        let error = Archive::clone_sync_remote(remote.to_str().unwrap(), &destination).unwrap_err();

        assert!(matches!(error, Error::GitCloneFailed { .. }));
        assert!(!destination.exists());
    }

    #[test]
    fn configures_and_clears_device_local_remote() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let remote = bare_remote(temporary.path());
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        assert_eq!(archive.sync_remote().unwrap().as_deref(), remote.to_str());
        assert!(archive.clear_sync_remote().unwrap());
        assert_eq!(archive.sync_remote().unwrap(), None);
        assert!(!archive.clear_sync_remote().unwrap());
    }

    #[test]
    fn publishes_and_fast_forwards_between_devices() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let mut first = Archive::create(temporary.path().join("first")).unwrap();
        first.set_sync_remote(remote.to_str().unwrap()).unwrap();
        assert_eq!(first.sync().unwrap().outcome(), SyncOutcome::Published);

        let mut second = clone_archive(&remote, &temporary.path().join("second"));
        let document = first.create_document("from first").unwrap();
        first
            .checkpoint(CheckpointKind::Structural, Some("first change"))
            .unwrap();
        assert_eq!(first.sync().unwrap().outcome(), SyncOutcome::Published);

        assert_eq!(
            second.sync().unwrap().outcome(),
            SyncOutcome::UpdatedFromRemote
        );
        assert_eq!(
            second.read_document(document).unwrap().content(),
            "from first"
        );
        assert_eq!(second.sync().unwrap().outcome(), SyncOutcome::Synced);
    }

    #[test]
    fn merges_non_conflicting_device_changes() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let mut first = Archive::create(temporary.path().join("first")).unwrap();
        first.set_sync_remote(remote.to_str().unwrap()).unwrap();
        first.sync().unwrap();
        let mut second = clone_archive(&remote, &temporary.path().join("second"));

        let first_document = first.create_document("first").unwrap();
        first
            .checkpoint(CheckpointKind::Structural, Some("first device"))
            .unwrap();

        let second_document = second.create_document("second").unwrap();
        second
            .checkpoint(CheckpointKind::Structural, Some("second device"))
            .unwrap();

        first.sync().unwrap();
        assert_eq!(second.sync().unwrap().outcome(), SyncOutcome::Merged);
        assert_eq!(
            second.read_document(first_document).unwrap().content(),
            "first"
        );
        assert_eq!(
            second.read_document(second_document).unwrap().content(),
            "second"
        );
    }

    #[test]
    fn reports_conflict_without_touching_local_worktree() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let mut first = Archive::create(temporary.path().join("first")).unwrap();
        let document = first.create_document("base").unwrap();
        first
            .checkpoint(CheckpointKind::Structural, Some("base document"))
            .unwrap();
        first.set_sync_remote(remote.to_str().unwrap()).unwrap();
        first.sync().unwrap();
        let mut second = clone_archive(&remote, &temporary.path().join("second"));

        first.edit_document(document, "first version").unwrap();
        first
            .checkpoint(CheckpointKind::Structural, Some("first edit"))
            .unwrap();
        second.edit_document(document, "second version").unwrap();
        second
            .checkpoint(CheckpointKind::Structural, Some("second edit"))
            .unwrap();

        first.sync().unwrap();
        assert_eq!(second.sync().unwrap().outcome(), SyncOutcome::Conflict);
        assert_eq!(
            second.read_document(document).unwrap().content(),
            "second version"
        );
        assert!(!second.is_dirty().unwrap());
    }

    #[test]
    fn rejects_a_remote_from_another_archive() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());

        let mut first = Archive::create(temporary.path().join("first")).unwrap();
        first.set_sync_remote(remote.to_str().unwrap()).unwrap();
        first.sync().unwrap();

        let mut other = Archive::create(temporary.path().join("other")).unwrap();
        other.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let error = other.sync().unwrap_err();
        assert!(matches!(error, Error::SyncArchiveMismatch { .. }));
    }

    #[test]
    fn refuses_non_carta_remote_with_existing_branch() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let seed = temporary.path().join("seed");
        std::fs::create_dir(&seed).unwrap();
        run_git(&seed, &["init", "--quiet"]);
        std::fs::write(seed.join("README"), "unrelated").unwrap();
        run_git(&seed, &["add", "README"]);
        run_git(
            &seed,
            &[
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--quiet",
                "-m",
                "seed",
            ],
        );
        run_git(&seed, &["branch", "-M", "main"]);
        run_git(
            &seed,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        run_git(&seed, &["push", "--quiet", "origin", "main"]);

        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        assert!(matches!(archive.sync(), Err(Error::SyncRemoteNotEmpty)));
    }
}
