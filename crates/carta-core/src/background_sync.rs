//! Off-thread, frontend-neutral remote synchronization.
//!
//! Network Git commands operate ONLY on an isolated, private clone of an
//! immutable local checkpoint. The live Archive is never changed by a worker.
//! The frontend may integrate the result only when its in-memory editor is
//! clean, the live working tree is clean and its HEAD still matches the base.
//! Never use this as a generic concurrent git-sync of the live worktree.
use crate::{Archive, Error, SyncOutcome};
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
        return Err(Error::InvalidSyncRemote("Synchronization is not configured".into()));
    }
    let base_head = git_text(&source, "inspect background-sync base", &[
        "rev-parse", "--verify", "HEAD",
    ])?;
    if crate::history::is_dirty_at(&source)? {
        return Err(Error::SyncRequiresCleanArchive);
    }
    // Most sync cycles need no clone. Inspect the remote only on this
    // worker, and publish the *pinned commit* (never a moving HEAD) when a
    // fast-forward is accepted by the Git server. Failure falls back to a
    // private merge snapshot; neither path modifies the live working tree.
    let remote_heads = crate::history::git_output(
        &source, "inspect background sync remote heads",
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
    // Git's non-force push rejects stale remote heads and concurrently
    // advanced branches. Retrying through the merge path is then safe.
    let refspec = format!("{base_head}:refs/heads/carta");
    let pushed = Command::new("git")
        .current_dir(&source)
        .args(["push", "--quiet", "carta-sync", &refspec])
        .output()
        .map_err(|error| Error::io(&source, error))?;
    if pushed.status.success() {
        return Ok(simple_stage(source, remote, base_head, SyncOutcome::Published));
    }

    // tempfile creates the staging directory with owner-only access (0700
    // on Unix), and cleans it automatically when no response retains it.
    let private = tempfile::tempdir().map_err(|e| Error::io(&source, e))?;
    let clone = private.path().join("archive");
    let status = Command::new("git")
        .args(["clone", "--quiet", "--no-hardlinks", "--single-branch",
               "--branch", "carta", "--"])
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
    let copied_head = git_text(&clone, "verify cloned base", &[
        "rev-parse", "--verify", "HEAD",
    ])?;
    if copied_head != base_head {
        return Err(Error::InvalidSyncRemote(
            "Archive changed while preparing background sync; retry later".into(),
        ));
    }

    let mut staged_archive = Archive::open(&clone)?;
    staged_archive.set_sync_remote(&remote)?;
    let outcome = staged_archive.sync()?.outcome();
    let synced_head = git_text(&clone, "read synchronized snapshot", &[
        "rev-parse", "--verify", "HEAD",
    ])?;

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
        if git_text(archive.root(), "check live sync base", &[
            "rev-parse", "--verify", "HEAD",
        ])? != stage.base_head {
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
            archive.root(), "import staged background sync result",
            &["fetch", "--quiet", "--no-tags", "--", clone.as_ref(), "refs/heads/carta"],
        )?;
        let fetched = git_text(archive.root(), "verify staged background sync result", &[
            "rev-parse", "--verify", "FETCH_HEAD",
        ])?;
        if fetched != stage.synced_head {
            return Err(Error::InvalidSyncRemote(
                "Background sync fetched an unexpected commit".into(),
            ));
        }
        // Recheck after object transfer: a local checkpoint could have been
        // created since the initial test.
        if archive.is_dirty()?
            || git_text(archive.root(), "recheck sync base", &[
                "rev-parse", "--verify", "HEAD",
            ])? != stage.base_head
        {
            return Ok(SyncApply::Stale);
        }
        crate::history::git_output(
            archive.root(), "install background sync checkpoint",
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
        assert!(Command::new("git").current_dir(path).args(args).status().unwrap().success());
    }

    #[test]
    fn stages_and_applies_remote_update_without_mutating_live_archive_in_worker() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(tmp.path(), &["init", "--bare", "--quiet", remote.to_str().unwrap()]);
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
        assert_eq!(git_text(&original_path, "test head", &["rev-parse", "HEAD"]).unwrap(), old_head);
        assert_eq!(staged.apply(&mut original).unwrap(), SyncApply::Updated);
        assert!(original.document_info(one).is_some());
        assert!(original.documents().count() >= 2);
    }

    #[test]
    fn private_worker_publishes_local_checkpoint_without_resetting_source() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(tmp.path(), &["init", "--bare", "--quiet", remote.to_str().unwrap()]);
        let root = tmp.path().join("local");
        let mut archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        archive.create_document("written on the first computer\n").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        let head = git_text(&root, "head", &["rev-parse", "HEAD"]).unwrap();
        let job = stage_sync(root.clone()).unwrap();
        assert_eq!(job.outcome(), SyncOutcome::Published);
        assert_eq!(git_text(&root, "head", &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(job.apply(&mut archive).unwrap(), SyncApply::Unchanged);
        let published = git_text(&remote, "remote published head", &[
            "rev-parse", "refs/heads/carta",
        ]).unwrap();
        assert_eq!(head, published);
    }

    #[test]
    fn uncheckpointed_local_writing_is_never_overwritten_by_old_sync_result() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(tmp.path(), &["init", "--bare", "--quiet", remote.to_str().unwrap()]);
        let root = tmp.path().join("local");
        let mut archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let doc = archive.create_document("old\n").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        archive.sync().unwrap();
        let staged = stage_sync(root.clone()).unwrap();
        archive.edit_document(doc, "writing in progress\n").unwrap();
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        assert_eq!(archive.read_document(doc).unwrap().content(), "writing in progress\n");
    }

    #[test]
    fn stale_snapshot_does_not_replace_new_local_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = tmp.path().join("remote.git");
        git(tmp.path(), &["init", "--bare", "--quiet", remote.to_str().unwrap()]);
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
