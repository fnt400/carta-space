//! Off-thread, frontend-neutral remote synchronization.
//!
//! Network Git commands operate ONLY on an isolated, private clone of an
//! immutable local checkpoint. The live Archive is never changed by a worker.
//! The frontend may integrate the result only when its in-memory editor is
//! clean, the live working tree is clean and its HEAD still matches the base.
//! Never use this as a generic concurrent git-sync of the live worktree.
use crate::{Archive, Error, SyncOutcome};
use carta_format::ArchiveMetadata;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Clone, Debug)]
pub struct StagedSync {
    data: Arc<Stage>,
}

#[derive(Debug)]
struct Stage {
    source: PathBuf,
    remote: String,
    base_head: String,
    synced_head: String,
    outcome: SyncOutcome,
    _private_directory: Option<TempDir>,
    clone: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncApply {
    Unchanged,
    Updated,
    Conflict,
    Stale,
}

/// A network job can run in spawn_blocking or a dedicated std::thread.
/// It takes a path, not an Archive reference, and NEVER writes into that path.
pub fn stage_sync(source: PathBuf) -> Result<StagedSync, Error> {
    // Read only Git config: opening a second Archive instance would run
    // recovery procedures on the active editor's live working tree.
    let config = Command::new("git")
        .current_dir(&source)
        .args(["config", "--get", "remote.carta-sync.url"])
        .output()
        .map_err(|error| Error::io(&source, error))?;
    let remote = if config.status.success() {
        String::from_utf8_lossy(&config.stdout).trim().to_owned()
    } else {
        String::new()
    };

    if remote.is_empty() {
        return Err(Error::InvalidSyncRemote(
            "Synchronization is not configured".into(),
        ));
    }
    let base_head = git_text(
        &source,
        "inspect background-sync base",
        &["rev-parse", "--verify", "HEAD"],
    )?;
    // Only immutable committed data may be published. The writer can keep
    // editing the live worktree while the worker pushes this pinned HEAD.
    // Importing remote changes is independently guarded by StagedSync::apply.
    // Most sync cycles need no clone. Inspect the remote only on this
    // worker, and publish the *pinned commit* (never a moving HEAD) when a
    // fast-forward is accepted by the Git server. Failure falls back to a
    // private merge snapshot; neither path modifies the live working tree.
    let remote_heads = crate::history::git_output(
        &source,
        "inspect background sync remote heads",
        &["ls-remote", "--heads", "carta-sync"],
    )?;
    let remote_heads = String::from_utf8_lossy(&remote_heads.stdout);
    let mut any_remote = false;
    let mut remote_head: Option<&str> = None;
    for line in remote_heads.lines() {
        any_remote = true;
        let mut fields = line.split_whitespace();
        let hash = fields.next().unwrap_or_default();
        if fields.next() == Some("refs/heads/carta") {
            remote_head = Some(hash);
        }
    }
    if any_remote && remote_head.is_none() {
        return Err(Error::SyncRemoteNotEmpty);
    }
    if remote_head == Some(base_head.as_str()) {
        return Ok(simple_stage(source, remote, base_head, SyncOutcome::Synced));
    }
    // The ordinary Archive::sync path validates Archive IDs before a
    // nonempty-remote push. The fast path must preserve that invariant.
    // When the remote commit is unknown locally, do not push optimistically:
    // use the isolated clone and the existing validated merge path instead.
    let can_fast_push = match remote_head {
        Some(remote_commit) => {
            let object = format!("{remote_commit}^{{commit}}");
            let present = Command::new("git")
                .current_dir(&source)
                .args(["cat-file", "-e", &object])
                .status()
                .map_err(|error| Error::io(&source, error))?
                .success();
            if present {
                validate_committed_archive_ids(&source, &base_head, remote_commit)?;
            }
            present
        }
        None => true, // Truly empty remote; no existing Archive to replace.
    };
    if can_fast_push {
        // Git's non-force push rejects stale remote heads and concurrently
        // advanced branches. Retrying through the merge path is then safe.
        let refspec = format!("{base_head}:refs/heads/carta");
        let pushed = Command::new("git")
            .current_dir(&source)
            .args(["push", "--quiet", "carta-sync", &refspec])
            .output()
            .map_err(|error| Error::io(&source, error))?;
        if pushed.status.success() {
            return Ok(simple_stage(
                source,
                remote,
                base_head,
                SyncOutcome::Published,
            ));
        }
    }

    // tempfile creates the staging directory with owner-only access (0700
    // on Unix), and cleans it automatically when no response retains it.
    let private = tempfile::tempdir().map_err(|e| Error::io(&source, e))?;
    let clone = private.path().join("archive");
    let status = Command::new("git")
        // The *remote* branch is called carta. The local Archive branch
        // may have a different name, so clone its actual checked-out HEAD.
        .args(["clone", "--quiet", "--no-hardlinks", "--"])
        .arg(&source)
        .arg(&clone)
        .status()
        .map_err(|source_error| Error::GitUnavailable {
            path: source.clone(),
            source: source_error,
        })?;
    if !status.success() {
        return Err(Error::InvalidSyncRemote(
            "Cannot prepare private background synchronization snapshot".into(),
        ));
    }
    // A local edit could have happened during clone. A stale job cannot
    // ever be applied; reject it now if clone's HEAD differs.
    let copied_head = git_text(
        &clone,
        "verify cloned base",
        &["rev-parse", "--verify", "HEAD"],
    )?;
    if copied_head != base_head {
        return Err(Error::InvalidSyncRemote(
            "Archive changed while preparing background sync; retry later".into(),
        ));
    }

    let mut staged_archive = Archive::open(&clone)?;
    staged_archive.set_sync_remote(&remote)?;
    let outcome = staged_archive.sync()?.outcome();
    let synced_head = git_text(
        &clone,
        "read synchronized snapshot",
        &["rev-parse", "--verify", "HEAD"],
    )?;

    Ok(StagedSync {
        data: Arc::new(Stage {
            source,
            remote,
            base_head,
            synced_head,
            outcome,
            _private_directory: Some(private),
            clone: Some(clone),
        }),
    })
}

/// Validate the immutable source and remote Git objects before a fast push.
/// This function never reads mutable working files or changes Git refs.
fn validate_committed_archive_ids(
    source: &Path,
    local_head: &str,
    remote_head: &str,
) -> Result<(), Error> {
    let read = |commit: &str| -> Result<ArchiveMetadata, Error> {
        let object = format!("{commit}:carta.json");
        let output = crate::history::git_output(
            source,
            "read committed Archive identity",
            &["show", &object],
        )?;
        ArchiveMetadata::read_from(Cursor::new(output.stdout))
            .map_err(|error| Error::InvalidSyncRemote(error.to_string()))
    };
    let local = read(local_head)?.archive_id();
    let remote = read(remote_head)?.archive_id();
    if local != remote {
        return Err(Error::SyncArchiveMismatch {
            local: local.to_string(),
            remote: remote.to_string(),
        });
    }
    Ok(())
}

fn simple_stage(
    source: PathBuf,
    remote: String,
    base_head: String,
    outcome: SyncOutcome,
) -> StagedSync {
    StagedSync {
        data: Arc::new(Stage {
            source,
            remote,
            synced_head: base_head.clone(),
            base_head,
            outcome,
            _private_directory: None,
            clone: None,
        }),
    }
}

fn git_text(root: &Path, operation: &'static str, args: &[&str]) -> Result<String, Error> {
    let result = crate::history::git_output(root, operation, args)?;
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}

impl StagedSync {
    pub fn outcome(&self) -> SyncOutcome {
        self.data.outcome
    }

    /// No networking. The caller MUST also check the live editor's dirty
    /// state immediately before calling. Local Git operations and Archive
    /// refresh still occur synchronously; schedule integration when idle.
    pub fn apply(&self, archive: &mut Archive) -> Result<SyncApply, Error> {
        let stage = &self.data;
        if archive.root() != stage.source {
            return Ok(SyncApply::Stale);
        }
        if archive.sync_remote()?.as_deref() != Some(stage.remote.as_str()) {
            return Ok(SyncApply::Stale);
        }
        if archive.is_dirty()? {
            return Ok(SyncApply::Stale);
        }
        if git_text(
            archive.root(),
            "check live sync base",
            &["rev-parse", "--verify", "HEAD"],
        )? != stage.base_head
        {
            return Ok(SyncApply::Stale);
        }
        if stage.outcome == SyncOutcome::Conflict {
            // Neither local nor remote has been reset. The manual conflict
            // resolver retains control; no discarded in-memory document.
            return Ok(SyncApply::Conflict);
        }
        if stage.synced_head == stage.base_head {
            return Ok(SyncApply::Unchanged);
        }

        // Transfer objects from the private snapshot without any remote SSH
        // operations. The staged HEAD must be the exact object produced by
        // the worker; never use a potentially moving remote branch ref.
        let Some(clone_path) = stage.clone.as_ref() else {
            return Err(Error::InvalidSyncRemote(
                "Background synchronization has no staged repository to import".into(),
            ));
        };
        let clone = clone_path.to_string_lossy();
        crate::history::git_output(
            archive.root(),
            "import staged background sync result",
            // The staging clone retains the local Archive's branch name, which
            // need not be "carta". Always import its pinned checked-out HEAD.
            &[
                "fetch",
                "--quiet",
                "--no-tags",
                "--",
                clone.as_ref(),
                "HEAD",
            ],
        )?;
        let fetched = git_text(
            archive.root(),
            "verify staged background sync result",
            &["rev-parse", "--verify", "FETCH_HEAD"],
        )?;
        if fetched != stage.synced_head {
            return Err(Error::InvalidSyncRemote(
                "Background sync fetched an unexpected commit".into(),
            ));
        }
        // Recheck after object transfer: a local checkpoint could have been
        // created since the initial test.
        if archive.is_dirty()?
            || git_text(
                archive.root(),
                "recheck sync base",
                &["rev-parse", "--verify", "HEAD"],
            )? != stage.base_head
        {
            return Ok(SyncApply::Stale);
        }
        crate::history::git_output(
            archive.root(),
            "install background sync checkpoint",
            &["reset", "--hard", "--quiet", &stage.synced_head],
        )?;
        archive.refresh()?;
        Ok(SyncApply::Updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckpointKind, SyncOutcome};
    use std::process::Command;

    fn git(path: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .current_dir(path)
            .args(args)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn stages_and_applies_remote_update_without_mutating_live_archive_in_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let original_path = tmp.path().join("original");
        let mut original = Archive::create(&original_path).unwrap();
        let url = remote.to_str().unwrap();
        original.set_sync_remote(url).unwrap();
        let one = original.create_document("first\n").unwrap();
        original.checkpoint(CheckpointKind::Manual, None).unwrap();
        original.sync().unwrap();

        let second_path = tmp.path().join("second");
        let mut second = Archive::clone_sync_remote(url, &second_path).unwrap();
        second.create_document("from second\n").unwrap();
        second.checkpoint(CheckpointKind::Manual, None).unwrap();
        second.sync().unwrap();

        let old_head = git_text(&original_path, "test head", &["rev-parse", "HEAD"]).unwrap();
        let staged = stage_sync(original_path.clone()).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::UpdatedFromRemote);
        assert_eq!(
            git_text(&original_path, "test head", &["rev-parse", "HEAD"]).unwrap(),
            old_head
        );
        assert_eq!(staged.apply(&mut original).unwrap(), SyncApply::Updated);
        assert!(original.document_info(one).is_some());
        assert!(original.documents().count() >= 2);
    }

    #[test]
    fn fast_path_rejects_different_archive_id_even_with_shared_git_history() {
        use crate::{ArchiveId, Timestamp};
        use std::fs::File;

        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = tmp.path().join("local");
        let archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        archive.sync_remote().unwrap().unwrap();
        let original_head = git_text(&root, "original head", &["rev-parse", "HEAD"]).unwrap();
        let initial = stage_sync(root.clone()).unwrap();
        assert_eq!(initial.outcome(), SyncOutcome::Published);

        // Simulate a local metadata identity change committed on top of the
        // published history. The old fast-path push would accept and publish
        // this unrelated Archive ID because Git ancestry alone matched.
        let replacement = ArchiveMetadata::new(ArchiveId::new_v7(), Timestamp::now_local());
        replacement
            .write_to(File::create(root.join("carta.json")).unwrap())
            .unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        let changed_head = git_text(&root, "changed head", &["rev-parse", "HEAD"]).unwrap();
        assert_ne!(changed_head, original_head);
        let error = stage_sync(root.clone()).unwrap_err();
        assert!(
            matches!(error, Error::SyncArchiveMismatch { .. }),
            "{error:?}"
        );
        let published = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["rev-parse", "refs/heads/carta"])
            .output()
            .unwrap();
        assert!(published.status.success());
        assert_eq!(
            String::from_utf8(published.stdout).unwrap().trim(),
            original_head
        );
    }

    #[test]
    fn private_worker_publishes_local_checkpoint_without_resetting_source() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = tmp.path().join("local");
        let mut archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        archive
            .create_document("written on the first computer\n")
            .unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        let head = git_text(&root, "head", &["rev-parse", "HEAD"]).unwrap();
        let job = stage_sync(root.clone()).unwrap();
        assert_eq!(job.outcome(), SyncOutcome::Published);
        assert_eq!(
            git_text(&root, "head", &["rev-parse", "HEAD"]).unwrap(),
            head
        );
        assert_eq!(job.apply(&mut archive).unwrap(), SyncApply::Unchanged);
        // The remote is bare: history::git_output assumes a worktree's
        // .git directory and is therefore intentionally not used here.
        let published = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["rev-parse", "refs/heads/carta"])
            .output()
            .unwrap();
        assert!(published.status.success());
        let published = String::from_utf8(published.stdout)
            .unwrap()
            .trim()
            .to_owned();
        assert_eq!(head, published);
    }

    #[test]
    fn uncheckpointed_local_writing_is_never_overwritten_by_old_sync_result() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = tmp.path().join("local");
        let mut archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let doc = archive.create_document("old\n").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        archive.sync().unwrap();
        let staged = stage_sync(root.clone()).unwrap();
        archive.edit_document(doc, "writing in progress\n").unwrap();
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        assert_eq!(
            archive.read_document(doc).unwrap().content(),
            "writing in progress\n"
        );
    }

    #[test]
    fn concurrent_edits_to_different_documents_merge_without_overwriting_local_work() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = tmp.path().join("fermi");
        let mut a = Archive::create(&root).unwrap();
        a.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let first = a.create_document("A before\n").unwrap();
        let second = a.create_document("B before\n").unwrap();
        a.checkpoint(CheckpointKind::Manual, None).unwrap();
        a.sync().unwrap();

        let peer = tmp.path().join("tanaka");
        let mut b = Archive::clone_sync_remote(remote.to_str().unwrap(), &peer).unwrap();
        a.edit_document(first, "A from Fermi\n").unwrap();
        a.checkpoint(CheckpointKind::Manual, None).unwrap();
        b.edit_document(second, "B from Tanaka\n").unwrap();
        b.checkpoint(CheckpointKind::Manual, None).unwrap();
        b.sync().unwrap();

        let local_before = a.read_document(first).unwrap().content().to_owned();
        let staged = stage_sync(root.clone()).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::Merged);
        assert_eq!(a.read_document(first).unwrap().content(), local_before);
        assert_eq!(staged.apply(&mut a).unwrap(), SyncApply::Updated);
        assert_eq!(a.read_document(first).unwrap().content(), "A from Fermi\n");
        assert_eq!(
            a.read_document(second).unwrap().content(),
            "B from Tanaka\n"
        );
    }

    #[test]
    fn concurrent_changes_to_same_document_are_never_silently_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = tmp.path().join("fermi");
        let mut a = Archive::create(&root).unwrap();
        a.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let doc = a.create_document("shared\n").unwrap();
        a.checkpoint(CheckpointKind::Manual, None).unwrap();
        a.sync().unwrap();

        let peer = tmp.path().join("tanaka");
        let mut b = Archive::clone_sync_remote(remote.to_str().unwrap(), &peer).unwrap();
        a.edit_document(doc, "Fermi\n").unwrap();
        a.checkpoint(CheckpointKind::Manual, None).unwrap();
        b.edit_document(doc, "Tanaka\n").unwrap();
        b.checkpoint(CheckpointKind::Manual, None).unwrap();
        b.sync().unwrap();

        let local_head = git_text(&root, "before conflict", &["rev-parse", "HEAD"]).unwrap();
        let staged = stage_sync(root.clone()).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::Conflict);
        assert_eq!(staged.apply(&mut a).unwrap(), SyncApply::Conflict);
        assert_eq!(
            git_text(&root, "after conflict", &["rev-parse", "HEAD"]).unwrap(),
            local_head
        );
        assert_eq!(a.read_document(doc).unwrap().content(), "Fermi\n");
        assert_eq!(b.read_document(doc).unwrap().content(), "Tanaka\n");
    }

    #[test]
    fn uncommitted_writing_does_not_hold_back_an_earlier_committed_push() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(tmp.path(), &["init", "--bare", "--quiet", remote.to_str().unwrap()]);
        let root = tmp.path().join("working");
        let mut archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let document = archive.create_document("committed text\n").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        let checkpoint_head =
            git_text(&root, "committed head", &["rev-parse", "HEAD"]).unwrap();

        // A later edit remains uncommitted while the asynchronous worker runs.
        archive.edit_document(document, "new uncommitted text\n").unwrap();
        assert!(archive.is_dirty().unwrap());
        let job = stage_sync(root.clone()).unwrap();
        assert_eq!(job.outcome(), SyncOutcome::Published);
        assert_eq!(job.apply(&mut archive).unwrap(), SyncApply::Stale);
        assert_eq!(
            archive.read_document(document).unwrap().content(),
            "new uncommitted text\n"
        );
        let published = Command::new("git")
            .arg("--git-dir")
            .arg(&remote)
            .args(["rev-parse", "refs/heads/carta"])
            .output()
            .unwrap();
        assert!(published.status.success());
        assert_eq!(
            String::from_utf8(published.stdout).unwrap().trim(),
            checkpoint_head
        );
    }

    #[test]
    fn stale_snapshot_does_not_replace_new_local_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let local = tmp.path().join("local");
        let mut archive = Archive::create(&local).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        archive.sync().unwrap();
        let staged = stage_sync(local.clone()).unwrap();
        archive.create_document("new local writing\n").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
    }
}
