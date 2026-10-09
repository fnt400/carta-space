//! Off-thread, frontend-neutral remote synchronization.
//!
//! Network Git commands operate ONLY in an isolated, private transport or
//! clone of an immutable checkpoint. The live Archive is never changed by a worker.
//! The frontend may integrate the result only when its in-memory editor is
//! clean, the live working tree is clean and its HEAD still matches the base.
//! Never use this as a generic concurrent git-sync of the live worktree.
use crate::sync::{configured_sync_remote, optional_git_text, resolved_sync_urls, SyncEndpoints};
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
    push_urls: Option<String>,
    effective_urls: (String, Vec<String>),
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
    // Read config only: opening another Archive would run live recovery.
    let remote = configured_sync_remote(&source)?.ok_or_else(|| {
        Error::InvalidSyncRemote("Synchronization is disabled or not configured".into())
    })?;
    let push_urls = optional_git_text(
        &source,
        "read background sync push URLs",
        &["config", "--get-all", "remote.carta-sync.pushurl"],
    )?;
    let base_head = git_text(
        &source,
        "inspect background-sync base",
        &["rev-parse", "--verify", "HEAD"],
    )?;
    let endpoints = SyncEndpoints::resolve(&source)?;
    // Only immutable committed data may be published. The writer can keep
    // editing the live worktree while the worker pushes this pinned HEAD.
    // Importing remote changes is independently guarded by StagedSync::apply.
    // Most sync cycles need no clone. Inspect the remote only on this
    // worker, and publish the *pinned commit* (never a moving HEAD) when a
    // fast-forward is accepted by the Git server. Unknown objects or incoming
    // history require a private merge snapshot; neither path changes the live tree.
    let remote_head = endpoints.head(&source, &endpoints.fetch)?;
    let known = endpoints.validate(&source, &base_head, false)?;
    if known
        && remote_head.as_deref() == Some(base_head.as_str())
        && endpoints.pushes == [endpoints.fetch.clone()]
    {
        return Ok(simple_stage(
            source,
            remote,
            push_urls,
            (endpoints.fetch.clone(), endpoints.pushes.clone()),
            base_head,
            SyncOutcome::Synced,
        ));
    }
    // The ordinary Archive::sync path validates Archive IDs before a
    // nonempty-remote push. The fast path must preserve that invariant.
    // When the remote commit is unknown locally, do not push optimistically:
    // use the isolated clone and the existing validated merge path instead.
    let can_fast_push = known
        && match remote_head.as_deref() {
            Some(head) => crate::sync::is_ancestor(&source, head, &base_head)?,
            None => true,
        };
    if can_fast_push {
        // Rejected or partial pushes stay pending at the caller. A later job
        // can fetch/merge a concurrently advanced branch and retry non-force.
        endpoints.publish(&source, &base_head)?;
        return Ok(simple_stage(
            source,
            remote,
            push_urls,
            (endpoints.fetch.clone(), endpoints.pushes.clone()),
            base_head,
            SyncOutcome::Published,
        ));
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
    let outcome = staged_archive.sync_with_endpoints(&endpoints)?.outcome();
    let synced_head = git_text(
        &clone,
        "read synchronized snapshot",
        &["rev-parse", "--verify", "HEAD"],
    )?;

    Ok(StagedSync {
        data: Arc::new(Stage {
            source,
            remote,
            push_urls,
            effective_urls: (endpoints.fetch.clone(), endpoints.pushes.clone()),
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
    push_urls: Option<String>,
    effective_urls: (String, Vec<String>),
    base_head: String,
    outcome: SyncOutcome,
) -> StagedSync {
    StagedSync {
        data: Arc::new(Stage {
            source,
            remote,
            push_urls,
            effective_urls,
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

    /// Check only the publication configuration, without networking or checks
    /// of editor state, working files or HEAD. An outbound snapshot may be
    /// acknowledged while newer writing continues; this does not authorize
    /// installing incoming changes, which still requires `apply`'s guards.
    pub fn configuration_matches(&self, archive: &Archive) -> Result<bool, Error> {
        let stage = &self.data;
        Ok(archive.root() == stage.source
            && archive.sync_remote()?.as_deref() == Some(stage.remote.as_str())
            && optional_git_text(
                archive.root(),
                "check live sync push URLs",
                &["config", "--get-all", "remote.carta-sync.pushurl"],
            )? == stage.push_urls
            && resolved_sync_urls(archive.root())? == stage.effective_urls)
    }

    /// No networking. The caller MUST also check the live editor's dirty
    /// state immediately before calling. Local Git operations and Archive
    /// refresh still occur synchronously; schedule integration when idle.
    pub fn apply(&self, archive: &mut Archive) -> Result<SyncApply, Error> {
        let stage = &self.data;
        if !self.configuration_matches(archive)? || archive.is_dirty()? {
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
            // URL-based pushes do not advance Git's local remote-tracking ref.
            // Otherwise "git status" falsely reports unpublished commits even
            // after the remote acknowledged the exact checkpoint.
            if matches!(stage.outcome, SyncOutcome::Synced | SyncOutcome::Published)
                && stage.effective_urls.1.contains(&stage.effective_urls.0)
            {
                crate::sync::record_confirmed_publication(archive.root(), &stage.base_head)?;
            }
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
            || !self.configuration_matches(archive)?
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
    use carta_format::ArchiveMetadata;
    use std::process::Command;

    fn git(path: &Path, args: &[&str]) {
        assert!(Command::new("git")
            .current_dir(path)
            .args(args)
            .status()
            .unwrap()
            .success());
    }

    fn adopted_archive(parent: &Path, push_urls: &[&Path]) -> (Archive, PathBuf) {
        let remote = parent.join("fetch.git");
        git(
            parent,
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = parent.join("local");
        let archive = Archive::create(&root).unwrap();
        git(
            &root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        git(
            &root,
            &["push", "--quiet", "origin", "HEAD:refs/heads/carta"],
        );
        let branch = git_text(&root, "test branch", &["branch", "--show-current"]).unwrap();
        git(
            &root,
            &["config", &format!("branch.{branch}.remote"), "origin"],
        );
        git(
            &root,
            &[
                "config",
                &format!("branch.{branch}.merge"),
                "refs/heads/carta",
            ],
        );
        for url in push_urls {
            git(
                &root,
                &[
                    "config",
                    "--add",
                    "remote.origin.pushurl",
                    url.to_str().unwrap(),
                ],
            );
        }
        assert!(archive.enable_origin_sync_remote().unwrap());
        (archive, remote)
    }

    fn bare_head(remote: &Path) -> String {
        let output = Command::new("git")
            .current_dir(remote)
            .args(["rev-parse", "refs/heads/carta"])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn disabling_sync_invalidates_stage_even_when_raw_routing_remains() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut archive, fetch) = adopted_archive(tmp.path(), &[]);
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert!(staged.configuration_matches(&archive).unwrap());
        git(archive.root(), &["config", "carta.sync-disabled", "true"]);
        assert_eq!(
            git_text(
                archive.root(),
                "test raw URL is unchanged",
                &["config", "--get", "remote.carta-sync.url"]
            )
            .unwrap(),
            fetch.to_str().unwrap()
        );
        assert_eq!(archive.sync_remote().unwrap(), None);
        assert_eq!(archive.sync().unwrap().outcome(), SyncOutcome::Disabled);
        assert!(stage_sync(archive.root().to_path_buf()).is_err());
        assert!(!staged.configuration_matches(&archive).unwrap());
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        git(archive.root(), &["config", "carta.sync-disabled", "false"]);
        assert!(staged.configuration_matches(&archive).unwrap());
    }

    #[test]
    fn configuration_matching_ignores_newer_checkpoints_and_dirty_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let (mut archive, fetch) = adopted_archive(tmp.path(), &[]);
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert!(staged.configuration_matches(&archive).unwrap());
        let document = archive
            .create_document("synthetic newer checkpoint")
            .unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        assert!(staged.configuration_matches(&archive).unwrap());
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        archive
            .edit_document(document, "synthetic unsaved writing")
            .unwrap();
        assert!(archive.is_dirty().unwrap());
        assert!(staged.configuration_matches(&archive).unwrap());
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);

        git(
            archive.root(),
            &[
                "config",
                "remote.carta-sync.pushurl",
                fetch.to_str().unwrap(),
            ],
        );
        assert!(!staged.configuration_matches(&archive).unwrap());
        git(
            archive.root(),
            &["config", "--unset", "remote.carta-sync.pushurl"],
        );
        assert!(staged.configuration_matches(&archive).unwrap());
        git(
            archive.root(),
            &["config", "remote.carta-sync.url", "../fetch.git"],
        );
        assert!(!staged.configuration_matches(&archive).unwrap());
        git(
            archive.root(),
            &["config", "remote.carta-sync.url", fetch.to_str().unwrap()],
        );
        let rewrite = format!(
            "url.{}.insteadOf",
            tmp.path().join("rerouted.git").display()
        );
        git(
            archive.root(),
            &["config", &rewrite, fetch.to_str().unwrap()],
        );
        assert!(!staged.configuration_matches(&archive).unwrap());
        git(archive.root(), &["config", "--unset", &rewrite]);
        assert!(staged.configuration_matches(&archive).unwrap());
        let other = Archive::create(tmp.path().join("other")).unwrap();
        assert!(!staged.configuration_matches(&other).unwrap());
        archive.clear_sync_remote().unwrap();
        assert!(!staged.configuration_matches(&archive).unwrap());
    }

    #[test]
    fn matching_fetch_head_still_publishes_to_all_adopted_push_urls() {
        let tmp = tempfile::tempdir().unwrap();
        let push = tmp.path().join("push.git");
        let other_push = tmp.path().join("other-push.git");
        for destination in [&push, &other_push] {
            git(
                tmp.path(),
                &["init", "--bare", "--quiet", destination.to_str().unwrap()],
            );
        }
        let (mut archive, fetch) = adopted_archive(tmp.path(), &[&push, &other_push]);
        git(
            archive.root(),
            &["push", "--quiet", "carta-sync", "HEAD:refs/heads/carta"],
        );
        let old_head = bare_head(&push);
        archive.create_document("synthetic new checkpoint").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        git(
            archive.root(),
            &[
                "push",
                "--quiet",
                fetch.to_str().unwrap(),
                "HEAD:refs/heads/carta",
            ],
        );
        let head = bare_head(&fetch);
        assert_ne!(head, old_head);
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::Published);
        for destination in [&push, &other_push] {
            assert_eq!(bare_head(destination), head);
        }
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Unchanged);

        // Changes to the list, its order, or its presence invalidate the job.
        for urls in [vec![&push], vec![&other_push, &push], vec![]] {
            git(
                archive.root(),
                &["config", "--unset-all", "remote.carta-sync.pushurl"],
            );
            for url in &urls {
                git(
                    archive.root(),
                    &[
                        "config",
                        "--add",
                        "remote.carta-sync.pushurl",
                        url.to_str().unwrap(),
                    ],
                );
            }
            assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        }
    }

    #[test]
    fn missing_adopted_push_destination_never_publishes_to_fetch_or_reports_success() {
        for ahead in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let missing = tmp.path().join("missing.git");
            let (mut archive, fetch) = adopted_archive(tmp.path(), &[&missing]);
            let fetch_head = bare_head(&fetch);
            if ahead {
                archive
                    .create_document("synthetic unpublished checkpoint")
                    .unwrap();
                archive.checkpoint(CheckpointKind::Manual, None).unwrap();
            }
            let local_head =
                git_text(archive.root(), "test local head", &["rev-parse", "HEAD"]).unwrap();
            assert!(stage_sync(archive.root().to_path_buf()).is_err());
            assert_eq!(bare_head(&fetch), fetch_head);
            assert_eq!(
                git_text(
                    archive.root(),
                    "test unchanged local head",
                    &["rev-parse", "HEAD"]
                )
                .unwrap(),
                local_head
            );
        }
    }

    #[test]
    fn private_fallback_publishes_incoming_and_divergent_results_to_adopted_push_url() {
        for divergent in [false, true] {
            let tmp = tempfile::tempdir().unwrap();
            let push = tmp.path().join("push.git");
            git(
                tmp.path(),
                &["init", "--bare", "--quiet", push.to_str().unwrap()],
            );
            let (mut archive, fetch) = adopted_archive(tmp.path(), &[&push]);
            // Resolve local URLs in the live Archive, not the private clone.
            git(
                archive.root(),
                &["config", "remote.carta-sync.url", "../fetch.git"],
            );
            git(
                archive.root(),
                &["config", "remote.carta-sync.pushurl", "../push.git"],
            );
            let mut peer =
                Archive::clone_sync_remote(fetch.to_str().unwrap(), tmp.path().join("peer"))
                    .unwrap();
            peer.create_document("synthetic incoming checkpoint")
                .unwrap();
            peer.checkpoint(CheckpointKind::Manual, None).unwrap();
            peer.sync().unwrap();
            let fetch_head = bare_head(&fetch);
            if divergent {
                archive
                    .create_document("synthetic local checkpoint")
                    .unwrap();
                archive.checkpoint(CheckpointKind::Manual, None).unwrap();
            }
            let local_head =
                git_text(archive.root(), "test local head", &["rev-parse", "HEAD"]).unwrap();
            let staged = stage_sync(archive.root().to_path_buf()).unwrap();
            assert!(staged.data.clone.is_some());
            assert_eq!(
                staged.outcome(),
                if divergent {
                    SyncOutcome::Merged
                } else {
                    SyncOutcome::UpdatedFromRemote
                }
            );
            assert_eq!(bare_head(&push), staged.data.synced_head);
            assert_eq!(bare_head(&fetch), fetch_head);
            assert_eq!(
                git_text(
                    archive.root(),
                    "test worker preserves local head",
                    &["rev-parse", "HEAD"]
                )
                .unwrap(),
                local_head
            );
            // A changed destination also blocks integration of incoming data.
            git(
                archive.root(),
                &[
                    "config",
                    "remote.carta-sync.pushurl",
                    "synthetic-changed-destination",
                ],
            );
            assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
            git(
                archive.root(),
                &["config", "remote.carta-sync.pushurl", "../push.git"],
            );
            assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Updated);
        }
    }

    #[test]
    fn validates_all_destinations_before_any_publication() {
        for foreground in [false, true] {
            for other_archive in [false, true] {
                for known in [false, true] {
                    let tmp = tempfile::tempdir().unwrap();
                    let empty = tmp.path().join("empty.git");
                    let invalid = tmp.path().join("invalid.git");
                    for destination in [&empty, &invalid] {
                        git(
                            tmp.path(),
                            &["init", "--bare", "--quiet", destination.to_str().unwrap()],
                        );
                    }
                    let (mut archive, fetch) = adopted_archive(tmp.path(), &[&empty, &invalid]);
                    let foreign = Archive::create(tmp.path().join("foreign")).unwrap();
                    git(
                        foreign.root(),
                        &[
                            "push",
                            "--quiet",
                            invalid.to_str().unwrap(),
                            if other_archive {
                                "HEAD:refs/heads/carta"
                            } else {
                                "HEAD:refs/heads/main"
                            },
                        ],
                    );
                    if known {
                        git(
                            archive.root(),
                            &[
                                "fetch",
                                "--quiet",
                                invalid.to_str().unwrap(),
                                if other_archive {
                                    "refs/heads/carta"
                                } else {
                                    "refs/heads/main"
                                },
                            ],
                        );
                    }
                    let head = git_text(archive.root(), "test checkpoint", &["rev-parse", "HEAD"])
                        .unwrap();
                    let config = std::fs::read(archive.root().join(".git/config")).unwrap();
                    let refs = git_text(archive.root(), "test refs", &["show-ref"]).unwrap();
                    let error = if foreground {
                        archive.sync().unwrap_err()
                    } else {
                        stage_sync(archive.root().to_path_buf()).unwrap_err()
                    };
                    assert!(
                        if other_archive {
                            matches!(error, Error::SyncArchiveMismatch { .. })
                        } else {
                            matches!(error, Error::SyncRemoteNotEmpty)
                        },
                        "{error:?}"
                    );
                    assert_eq!(bare_head(&fetch), head);
                    let output = Command::new("git")
                        .current_dir(&empty)
                        .args(["show-ref"])
                        .output()
                        .unwrap();
                    assert!(
                        output.stdout.is_empty(),
                        "earlier valid destination was published"
                    );
                    assert_eq!(
                        git_text(archive.root(), "test checkpoint", &["rev-parse", "HEAD"])
                            .unwrap(),
                        head
                    );
                    assert_eq!(
                        std::fs::read(archive.root().join(".git/config")).unwrap(),
                        config
                    );
                    if !foreground {
                        assert_eq!(
                            git_text(archive.root(), "test refs", &["show-ref"]).unwrap(),
                            refs
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn default_push_urls_honor_push_instead_of_and_multiple_fetch_urls() {
        let tmp = tempfile::tempdir().unwrap();
        let push = tmp.path().join("push.git");
        let other = tmp.path().join("other.git");
        for destination in [&push, &other] {
            git(
                tmp.path(),
                &["init", "--bare", "--quiet", destination.to_str().unwrap()],
            );
        }
        let (mut archive, fetch) = adopted_archive(tmp.path(), &[]);
        git(
            archive.root(),
            &["config", "remote.carta-sync.url", "synthetic-fetch"],
        );
        git(
            archive.root(),
            &["config", "--add", "remote.carta-sync.url", "../other.git"],
        );
        git(
            archive.root(),
            &["config", "url.../fetch.git.insteadOf", "synthetic-fetch"],
        );
        git(
            archive.root(),
            &["config", "url.../push.git.pushInsteadOf", "synthetic-fetch"],
        );
        let endpoints = SyncEndpoints::resolve(archive.root()).unwrap();
        assert_eq!(
            endpoints.fetch,
            archive.root().join("../fetch.git").to_string_lossy()
        );
        assert_eq!(
            endpoints.pushes,
            vec![archive.root().join("../push.git").to_string_lossy()]
        );
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert!(staged.data.clone.is_none());
        assert_eq!(bare_head(&push), bare_head(&fetch));
        // Git synthesizes a pushurl for pushInsteadOf. Without that rule,
        // its default publication list includes every configured fetch URL.
        git(
            archive.root(),
            &["config", "--unset", "url.../push.git.pushInsteadOf"],
        );
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        let endpoints = SyncEndpoints::resolve(archive.root()).unwrap();
        assert_eq!(
            endpoints.pushes,
            vec![
                archive.root().join("../fetch.git").to_string_lossy(),
                archive.root().join("../other.git").to_string_lossy()
            ]
        );
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert!(staged.data.clone.is_none());
        assert_eq!(bare_head(&other), bare_head(&fetch));
        git(
            archive.root(),
            &[
                "config",
                "--replace-all",
                "remote.carta-sync.url",
                "../fetch.git",
            ],
        );
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
    }

    #[test]
    fn rejected_later_push_stays_pending_and_retries_without_force() {
        let tmp = tempfile::tempdir().unwrap();
        let push = tmp.path().join("push.git");
        let advanced = tmp.path().join("advanced.git");
        for destination in [&push, &advanced] {
            git(
                tmp.path(),
                &["init", "--bare", "--quiet", destination.to_str().unwrap()],
            );
        }
        let (mut archive, fetch) = adopted_archive(tmp.path(), &[&push, &advanced]);
        let mut peer =
            Archive::clone_sync_remote(fetch.to_str().unwrap(), tmp.path().join("peer")).unwrap();
        peer.create_document("synthetic independent remote checkpoint")
            .unwrap();
        peer.checkpoint(CheckpointKind::Manual, None).unwrap();
        git(
            peer.root(),
            &[
                "push",
                "--quiet",
                advanced.to_str().unwrap(),
                "HEAD:refs/heads/carta",
            ],
        );
        let remote_head = bare_head(&advanced);
        let local_head =
            git_text(archive.root(), "test checkpoint", &["rev-parse", "HEAD"]).unwrap();
        // Unknown destination metadata is fetched only in the private fallback.
        assert!(stage_sync(archive.root().to_path_buf()).is_err());
        assert_eq!(bare_head(&push), local_head);
        assert_eq!(bare_head(&advanced), remote_head);
        assert_eq!(
            git_text(archive.root(), "test checkpoint", &["rev-parse", "HEAD"]).unwrap(),
            local_head
        );
        peer.sync().unwrap();
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert_eq!(staged.outcome(), SyncOutcome::UpdatedFromRemote);
        assert_eq!(bare_head(&push), remote_head);
        assert_eq!(bare_head(&advanced), remote_head);
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Updated);
    }

    #[test]
    fn resolved_rewrites_are_pinned_and_changes_make_stage_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let push = tmp.path().join("push.git");
        let rerouted = tmp.path().join("rerouted.git");
        for destination in [&push, &rerouted] {
            git(
                tmp.path(),
                &["init", "--bare", "--quiet", destination.to_str().unwrap()],
            );
        }
        let (mut archive, _) = adopted_archive(tmp.path(), &[]);
        git(
            archive.root(),
            &["config", "remote.carta-sync.url", "synthetic-fetch"],
        );
        git(
            archive.root(),
            &["config", "remote.carta-sync.pushurl", "synthetic-push"],
        );
        git(
            archive.root(),
            &["config", "url.../fetch.git.insteadOf", "synthetic-fetch"],
        );
        git(
            archive.root(),
            &["config", "url.../push.git.insteadOf", "synthetic-push"],
        );
        let endpoints = SyncEndpoints::resolve(archive.root()).unwrap();
        let head = git_text(archive.root(), "test checkpoint", &["rev-parse", "HEAD"]).unwrap();
        assert!(endpoints.validate(archive.root(), &head, false).unwrap());
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        assert!(staged.data.clone.is_none());
        git(
            archive.root(),
            &["config", "url.../push.git.insteadOf", "unused-prefix"],
        );
        git(
            archive.root(),
            &["config", "url.../rerouted.git.insteadOf", "synthetic-push"],
        );
        archive
            .create_document("synthetic pinned publication")
            .unwrap();
        let absolute_rewrite = format!("url.{}.insteadOf", rerouted.display());
        git(
            archive.root(),
            &["config", &absolute_rewrite, &endpoints.pushes[0]],
        );
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        let new_head = git_text(archive.root(), "test checkpoint", &["rev-parse", "HEAD"]).unwrap();
        endpoints.publish(archive.root(), &new_head).unwrap();
        assert_eq!(bare_head(&push), new_head);
        assert!(Command::new("git")
            .current_dir(&rerouted)
            .args(["show-ref"])
            .output()
            .unwrap()
            .stdout
            .is_empty());
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
        // Check effective routing independently of the raw config guards.
        git(archive.root(), &["config", "--unset", &absolute_rewrite]);
        git(
            archive.root(),
            &["config", "url.../push.git.insteadOf", "synthetic-push"],
        );
        git(
            archive.root(),
            &["config", "--unset", "url.../rerouted.git.insteadOf"],
        );
        let staged = stage_sync(archive.root().to_path_buf()).unwrap();
        git(
            archive.root(),
            &["config", "url.../push.git.insteadOf", "unused-prefix"],
        );
        git(
            archive.root(),
            &["config", "url.../rerouted.git.insteadOf", "synthetic-push"],
        );
        assert_eq!(staged.apply(&mut archive).unwrap(), SyncApply::Stale);
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
        git(
            tmp.path(),
            &["init", "--bare", "--quiet", remote.to_str().unwrap()],
        );
        let root = tmp.path().join("working");
        let mut archive = Archive::create(&root).unwrap();
        archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
        let document = archive.create_document("committed text\n").unwrap();
        archive.checkpoint(CheckpointKind::Manual, None).unwrap();
        let checkpoint_head = git_text(&root, "committed head", &["rev-parse", "HEAD"]).unwrap();

        // A later edit remains uncommitted while the asynchronous worker runs.
        archive
            .edit_document(document, "new uncommitted text\n")
            .unwrap();
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
