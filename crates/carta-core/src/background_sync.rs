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
    _private_directory: TempDir,
    clone: PathBuf,
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
            _private_directory: private,
            clone,
        }),
    })
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
        let clone = stage.clone.to_string_lossy();
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
