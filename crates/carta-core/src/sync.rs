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

/// Resolved once in the live repository. Network commands use a private Git
/// config snapshot without routing rules, so later URL/config changes cannot
/// redirect an already validated publication (including insteadOf rewrites).
pub(crate) struct SyncEndpoints {
    pub(crate) fetch: String,
    pub(crate) pushes: Vec<String>,
    transport: tempfile::TempDir,
}

impl SyncEndpoints {
    pub(crate) fn resolve(root: &Path) -> Result<Self, Error> {
        let (fetch, pushes) = resolved_sync_urls(root)?;
        let transport = tempfile::tempdir().map_err(|error| Error::io(root, error))?;
        let output = Command::new("git")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                transport.path().join("no-global-config"),
            )
            .args(["init", "--bare", "--quiet", "--template="])
            .arg(transport.path())
            .output()
            .map_err(|error| Error::io(root, error))?;
        if !output.status.success() {
            return Err(command_failed("prepare sync transport", output));
        }
        // Preserve authentication/SSH and other effective Git settings, but
        // never inherit URL rewrites, includes, remotes or worktree geometry.
        let config = crate::history::git_output(
            root,
            "snapshot sync transport config",
            &["config", "--null", "--list"],
        )?;
        for entry in config
            .stdout
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
        {
            let entry = std::str::from_utf8(entry)
                .map_err(|error| Error::InvalidSyncRemote(error.to_string()))?;
            let (key, value) = entry.split_once('\n').unwrap_or((entry, ""));
            if key != "core.sshcommand"
                && [
                    "url.",
                    "remote.",
                    "include.",
                    "includeif.",
                    "core.",
                    "extensions.",
                ]
                .iter()
                .any(|prefix| key.starts_with(prefix))
            {
                continue;
            }
            let output = Command::new("git")
                .current_dir(transport.path())
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env(
                    "GIT_CONFIG_GLOBAL",
                    transport.path().join("no-global-config"),
                )
                .args(["config", "--local", "--add", key, value])
                .output()
                .map_err(|error| Error::io(root, error))?;
            if !output.status.success() {
                return Err(command_failed("snapshot sync transport config", output));
            }
        }
        Ok(Self {
            fetch,
            pushes,
            transport,
        })
    }

    fn network(
        &self,
        root: &Path,
        operation: &'static str,
        args: &[&str],
    ) -> Result<Output, Error> {
        let objects = required_git_text(
            root,
            "locate sync objects",
            &["rev-parse", "--git-path", "objects"],
        )?;
        let objects =
            fs::canonicalize(root.join(objects)).map_err(|error| Error::io(root, error))?;
        // Local imports must see the same borrowed ancestors as the network
        // fetch. Only this disposable transport's object database is changed.
        let alternates = self.transport.path().join("objects/info/alternates");
        fs::write(&alternates, format!("{}\n", objects.display()))
            .map_err(|error| Error::io(&alternates, error))?;
        let output = Command::new("git")
            .current_dir(self.transport.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                self.transport.path().join("no-global-config"),
            )
            .args(args)
            .output()
            .map_err(|error| Error::io(root, error))?;
        if !output.status.success() {
            return Err(command_failed(operation, output));
        }
        Ok(output)
    }

    pub(crate) fn head(&self, root: &Path, url: &str) -> Result<Option<String>, Error> {
        let output = self.network(
            root,
            "inspect sync destination",
            &["ls-remote", "--refs", "--", url],
        )?;
        let text = text_output(output, "inspect sync destination")?;
        for line in text.lines() {
            let mut fields = line.split_whitespace();
            let hash = fields.next().unwrap_or_default();
            if fields.next() == Some("refs/heads/carta") {
                return Ok(Some(hash.to_owned()));
            }
        }
        if !text.is_empty() {
            return Err(Error::SyncRemoteNotEmpty);
        }
        Ok(None)
    }

    pub(crate) fn validate(
        &self,
        root: &Path,
        local: &str,
        fetch_unknown: bool,
    ) -> Result<bool, Error> {
        let mut known = true;
        for url in std::iter::once(&self.fetch).chain(&self.pushes) {
            if let Some(head) = self.head(root, url)? {
                let output =
                    raw_git_output(root, &["cat-file", "-e", &format!("{head}^{{commit}}")])?;
                if !output.status.success() {
                    if !fetch_unknown {
                        known = false;
                        continue;
                    }
                    self.fetch_into(root, url, &head, None)?;
                }
                validate_committed_archive_ids(root, local, &head)?;
            }
        }
        Ok(known)
    }

    fn fetch_into(
        &self,
        root: &Path,
        url: &str,
        head: &str,
        tracking: Option<&str>,
    ) -> Result<(), Error> {
        self.network(
            root,
            "fetch pinned sync history",
            &["fetch", "--quiet", "--no-tags", "--", url, head],
        )?;
        let transport = self.transport.path().to_string_lossy();
        let refspec = tracking
            .map(|reference| format!("+{head}:{reference}"))
            .unwrap_or_else(|| head.to_owned());
        crate::history::git_output(
            root,
            "import pinned sync history",
            &["fetch", "--quiet", "--no-tags", "--", &transport, &refspec],
        )?;
        Ok(())
    }

    pub(crate) fn publish(&self, root: &Path, head: &str) -> Result<(), Error> {
        // All destinations must have been validated before the first push.
        for url in &self.pushes {
            self.network(
                root,
                "push sync destination",
                &[
                    "push",
                    "--quiet",
                    "--",
                    url,
                    &format!("{head}:refs/heads/carta"),
                ],
            )?;
        }
        Ok(())
    }
}

/// The synchronous metadata reflection of an already acknowledged push.
/// URL-based Git push does not update refs/remotes/carta-sync/carta. Keeping
/// that ref stale makes "git status" claim the Archive is still ahead.
/// Never rewind a ref which another process may have advanced; use CAS.
pub(crate) fn record_confirmed_publication(root: &Path, head: &str) -> Result<(), Error> {
    let current = optional_git_text(
        root,
        "inspect sync tracking ref",
        &["rev-parse", "--verify", "--quiet", SYNC_TRACKING_REF],
    )?;
    if current.as_deref() == Some(head) {
        return Ok(());
    }
    if let Some(old) = current.as_deref() {
        if !is_ancestor(root, old, head)? {
            return Err(Error::InvalidSyncRemote(
                "Remote tracking ref changed unexpectedly; sync must be verified again".into(),
            ));
        }
    }
    let previous = current.unwrap_or_else(|| "0000000000000000000000000000000000000000".into());
    crate::history::git_output(
        root,
        "reflect confirmed sync publication",
        &["update-ref", SYNC_TRACKING_REF, head, &previous],
    )?;
    Ok(())
}

pub(crate) fn resolved_sync_urls(root: &Path) -> Result<(String, Vec<String>), Error> {
    let resolve = |args: &[&str]| -> Result<Vec<String>, Error> {
        let urls = required_git_text(root, "resolve sync URLs", args)?;
        let root = fs::canonicalize(root).map_err(|error| Error::io(root, error))?;
        Ok(urls
            .lines()
            .map(|url| {
                // Git's local-path form has neither a scheme nor SCP's colon.
                if !url.contains(':') && !Path::new(url).is_absolute() {
                    root.join(url).to_string_lossy().into_owned()
                } else {
                    url.to_owned()
                }
            })
            .collect())
    };
    Ok((
        resolve(&["remote", "get-url", SYNC_REMOTE])?.remove(0),
        resolve(&["remote", "get-url", "--push", "--all", SYNC_REMOTE])?,
    ))
}

pub(crate) fn validate_committed_archive_ids(
    root: &Path,
    local: &str,
    remote: &str,
) -> Result<(), Error> {
    let read = |head: &str| -> Result<ArchiveMetadata, Error> {
        let output = crate::history::git_output(
            root,
            "read committed Archive identity",
            &["show", &format!("{head}:carta.json")],
        )?;
        ArchiveMetadata::read_from(Cursor::new(output.stdout))
            .map_err(|error| Error::InvalidSyncRemote(error.to_string()))
    };
    let local = read(local)?.archive_id();
    let remote = read(remote)?.archive_id();
    if local != remote {
        return Err(Error::SyncArchiveMismatch {
            local: local.to_string(),
            remote: remote.to_string(),
        });
    }
    Ok(())
}

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
    pub fn clone_sync_remote(url: &str, destination: impl AsRef<Path>) -> Result<Self, Error> {
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
        configured_sync_remote(&self.root)
    }

    /// Opt in to adopting origin for a branch tracking origin/carta.
    /// Frontends call this explicitly; it changes only device-local Git config.
    /// Returns true only when a missing dedicated remote was configured.
    pub fn enable_origin_sync_remote(&self) -> Result<bool, Error> {
        let snapshot = sync_config_snapshot(&self.root)?;
        if self.sync_remote()?.is_some() || sync_disabled(&self.root)? {
            return Ok(false);
        }
        let Some(branch) = optional_git_text(
            &self.root,
            "read current branch for sync adoption",
            &["branch", "--show-current"],
        )?
        else {
            return Ok(false);
        };
        for (key, expected) in [
            (format!("branch.{branch}.remote"), "origin"),
            (format!("branch.{branch}.merge"), "refs/heads/carta"),
        ] {
            if optional_git_text(
                &self.root,
                "read branch upstream for sync adoption",
                &["config", "--local", "--get", &key],
            )?
            .as_deref()
                != Some(expected)
            {
                return Ok(false);
            }
        }
        let read_urls = |key: &str| -> Result<Vec<String>, Error> {
            let output = raw_git_output(&self.root, &["config", "--null", "--get-all", key])?;
            if output.status.code() == Some(1) && output.stdout.is_empty() {
                return Ok(Vec::new());
            }
            if !output.status.success() {
                return Err(command_failed("read origin URLs for sync adoption", output));
            }
            let text = text_output(output, "read origin URLs for sync adoption")?;
            Ok(text
                .strip_suffix('\0')
                .unwrap_or(&text)
                .split('\0')
                .map(str::to_owned)
                .collect())
        };
        let urls = read_urls("remote.origin.url")?;
        if urls.is_empty() || (urls.len() == 1 && urls[0].trim().is_empty()) {
            return Ok(false);
        }
        let push_urls = read_urls("remote.origin.pushurl")?;
        if urls
            .iter()
            .chain(&push_urls)
            .any(|url| url.trim().is_empty() || url.contains(['\n', '\r']))
        {
            return Err(Error::InvalidSyncRemote(
                "Origin has an empty or multiline synchronization URL".into(),
            ));
        }
        // Resolve before taking the lock, but copy every raw value so includes,
        // relative paths and Git's insteadOf/pushInsteadOf behavior survive.
        required_git_text(
            &self.root,
            "resolve origin fetch URL",
            &["remote", "get-url", "origin"],
        )?;
        required_git_text(
            &self.root,
            "resolve origin push URLs",
            &["remote", "get-url", "--push", "--all", "origin"],
        )?;

        let mut commands = vec![vec!["--unset-all", "remote.carta-sync.pushurl"]];
        for (key, values) in [
            ("remote.carta-sync.url", &urls),
            ("remote.carta-sync.pushurl", &push_urls),
        ] {
            for (index, value) in values.iter().enumerate() {
                commands.push(vec![
                    if index == 0 { "--replace-all" } else { "--add" },
                    key,
                    value,
                ]);
            }
        }
        commands.push(vec![
            "--replace-all",
            "remote.carta-sync.fetch",
            "+refs/heads/*:refs/remotes/carta-sync/*",
        ]);
        commands.push(vec!["--replace-all", "carta.sync-disabled", "false"]);
        update_sync_config(&self.root, &snapshot, Some(&branch), None, &commands)?;
        Ok(true)
    }

    pub fn set_sync_remote(&self, url: &str) -> Result<(), Error> {
        let url = url.trim();
        if url.is_empty() {
            return self.clear_sync_remote().map(|_| ());
        }
        let snapshot = sync_config_snapshot(&self.root)?;
        update_sync_config(
            &self.root,
            &snapshot,
            None,
            Some(url),
            &[
                vec!["--unset-all", "remote.carta-sync.pushurl"],
                vec!["--replace-all", "remote.carta-sync.url", url],
                vec![
                    "--replace-all",
                    "remote.carta-sync.fetch",
                    "+refs/heads/*:refs/remotes/carta-sync/*",
                ],
                vec!["carta.sync-disabled", "false"],
            ],
        )
    }

    pub fn clear_sync_remote(&self) -> Result<bool, Error> {
        let snapshot = sync_config_snapshot(&self.root)?;
        let configured = self.sync_remote()?.is_some();
        let mut commands = vec![vec!["--replace-all", "carta.sync-disabled", "true"]];
        if optional_git_text(
            &self.root,
            "inspect local sync section",
            &[
                "config",
                "--local",
                "--no-includes",
                "--get-regexp",
                "^remote\\.carta-sync\\.",
            ],
        )?
        .is_some()
        {
            commands.push(vec!["--remove-section", "remote.carta-sync"]);
        }
        update_sync_config(&self.root, &snapshot, None, None, &commands)?;
        if !configured {
            return Ok(false);
        }
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
        let endpoints = SyncEndpoints::resolve(&self.root)?;
        self.sync_with_endpoints(&endpoints)
    }

    pub(crate) fn sync_with_endpoints(
        &mut self,
        endpoints: &SyncEndpoints,
    ) -> Result<SyncReport, Error> {
        let local = current_head(&self.root)?;
        endpoints.validate(&self.root, &local, true)?;
        match endpoints.head(&self.root, &endpoints.fetch)? {
            None => {
                endpoints.publish(&self.root, &local)?;
                Ok(SyncReport::new(SyncOutcome::Published))
            }
            Some(head) => {
                endpoints.fetch_into(
                    &self.root,
                    &endpoints.fetch,
                    &head,
                    Some(SYNC_TRACKING_REF),
                )?;
                validate_committed_archive_ids(&self.root, &local, &head)?;

                let local = current_head(&self.root)?;
                let remote = current_tracking_head(&self.root)?;
                if local == remote {
                    if endpoints.pushes != [endpoints.fetch.clone()] {
                        endpoints.publish(&self.root, &local)?;
                    }
                    return Ok(SyncReport::new(SyncOutcome::Synced));
                }

                if is_ancestor(&self.root, &remote, &local)? {
                    endpoints.publish(&self.root, &local)?;
                    return Ok(SyncReport::new(SyncOutcome::Published));
                }

                if is_ancestor(&self.root, &local, &remote)? {
                    if endpoints.pushes != [endpoints.fetch.clone()] {
                        endpoints.publish(&self.root, &remote)?;
                    }
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
                        endpoints.publish(&self.root, &merge)?;
                        Ok(SyncReport::new(SyncOutcome::Merged))
                    }
                }
            }
        }
    }
}

fn sync_disabled(root: &Path) -> Result<bool, Error> {
    // A top-level device preference must not be undone by an included setting.
    let local = optional_git_text(
        root,
        "read local sync disable preference",
        &[
            "config",
            "--local",
            "--no-includes",
            "--bool",
            "--get",
            "carta.sync-disabled",
        ],
    )?;
    let value = match local {
        Some(value) => Some(value),
        None => optional_git_text(
            root,
            "read inherited sync disable preference",
            &["config", "--bool", "--get", "carta.sync-disabled"],
        )?,
    };
    Ok(value.as_deref() == Some("true"))
}

pub(crate) fn configured_sync_remote(root: &Path) -> Result<Option<String>, Error> {
    if sync_disabled(root)? {
        return Ok(None);
    }
    optional_git_text(
        root,
        "read sync remote",
        &["config", "--get", "remote.carta-sync.url"],
    )
}

fn sync_config_snapshot(root: &Path) -> Result<(Vec<u8>, Vec<u8>), Error> {
    let config = root.join(".git/config");
    Ok((
        fs::read(&config).map_err(|error| Error::io(&config, error))?,
        crate::history::git_output(
            root,
            "inspect synchronization configuration",
            &["config", "--null", "--list"],
        )?
        .stdout,
    ))
}

/// Publish all routing/preference changes together under Git's normal lock.
/// Snapshot comparisons prevent overwriting edits made before lock acquisition.
fn update_sync_config(
    root: &Path,
    snapshot: &(Vec<u8>, Vec<u8>),
    branch: Option<&str>,
    replacement_url: Option<&str>,
    commands: &[Vec<&str>],
) -> Result<(), Error> {
    let config_path = root.join(".git/config");
    let lock_path = root.join(".git/config.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| Error::io(&lock_path, error))?;
    let result: Result<(), Error> = (|| {
        let unchanged = || -> Result<(), Error> {
            if sync_config_snapshot(root)? != *snapshot
                || (branch.is_some()
                    && optional_git_text(
                        root,
                        "recheck branch for sync adoption",
                        &["branch", "--show-current"],
                    )?
                    .as_deref()
                        != branch)
            {
                return Err(Error::InvalidSyncRemote("Git configuration changed during synchronization configuration update; retry later".into()));
            }
            Ok(())
        };
        unchanged()?;
        // Git config --file must reopen the staging path in a child process.
        // Keep only a TempPath (no open Windows handle) while constructing it.
        // TempPath still removes an uncommitted staging file on failure.
        let staged = tempfile::NamedTempFile::new_in(root.join(".git"))
            .map_err(|error| Error::io(&config_path, error))?
            .into_temp_path();
        let staged_path = staged.to_path_buf();
        fs::write(&staged_path, &snapshot.0).map_err(|error| Error::io(&staged_path, error))?;
        for args in commands {
            let output = crate::history::git_command(root)
                .args(["config", "--file"])
                .arg(&staged_path)
                .args(args)
                .output()
                .map_err(|error| Error::io(&staged_path, error))?;
            if !output.status.success()
                && !(args.first() == Some(&"--unset-all") && output.status.code() == Some(5))
            {
                return Err(command_failed(
                    "prepare synchronization configuration",
                    output,
                ));
            }
        }
        if let Some(url) = replacement_url {
            // Local writes cannot remove values from global/included files.
            // Reject inherited routing, and also inspect the staged includes
            // in case changing the URL activates a conditional include.
            let local_config =
                fs::canonicalize(&config_path).map_err(|error| Error::io(&config_path, error))?;
            for key in ["remote.carta-sync.url", "remote.carta-sync.pushurl"] {
                // Project the requested URL into Git's full config evaluation
                // so global hasconfig includes cannot appear only after commit.
                let projected_url = format!("remote.carta-sync.url={url}");
                let output = raw_git_output(
                    root,
                    &[
                        "-c",
                        &projected_url,
                        "config",
                        "--show-origin",
                        "--null",
                        "--get-all",
                        key,
                    ],
                )?;
                if !output.status.success()
                    && !(output.status.code() == Some(1) && output.stdout.is_empty())
                {
                    return Err(command_failed("inspect inherited sync routing", output));
                }
                let text = text_output(output, "inspect inherited sync routing")?;
                let entries: Vec<_> = text.split_terminator('\0').collect();
                for (index, entry) in entries.chunks_exact(2).enumerate() {
                    let origin = entry[0];
                    if key == "remote.carta-sync.url"
                        && index * 2 + 2 == entries.len()
                        && origin == "command line:"
                        && entry[1] == url
                    {
                        continue;
                    }
                    let local = origin
                        .strip_prefix("file:")
                        .and_then(|path| fs::canonicalize(root.join(path)).ok())
                        .as_ref()
                        == Some(&local_config);
                    if !local {
                        return Err(Error::InvalidSyncRemote("Synchronization routing is inherited from another Git configuration file; remove it there before changing the remote".into()));
                    }
                }
                let output = crate::history::git_command(root)
                    .args(["config", "--file"])
                    .arg(&staged_path)
                    .args(["--includes", "--null", "--get-all", key])
                    .output()
                    .map_err(|error| Error::io(&staged_path, error))?;
                if !output.status.success()
                    && !(output.status.code() == Some(1) && output.stdout.is_empty())
                {
                    return Err(command_failed("verify replacement sync routing", output));
                }
                let expected = if key == "remote.carta-sync.url" {
                    format!("{url}\0").into_bytes()
                } else {
                    Vec::new()
                };
                if output.stdout != expected {
                    return Err(Error::InvalidSyncRemote("Included Git configuration prevents replacing synchronization routing; remove it there before changing the remote".into()));
                }
            }
        }
        fs::set_permissions(
            &staged_path,
            fs::metadata(&config_path)
                .map_err(|error| Error::io(&config_path, error))?
                .permissions(),
        )
        .map_err(|error| Error::io(&staged_path, error))?;
        fs::File::open(&staged_path)
            .and_then(|file| file.sync_all())
            .map_err(|error| Error::io(&staged_path, error))?;
        unchanged()?;
        staged
            .persist(&config_path)
            .map_err(|error| Error::io(&config_path, error.error))?;
        Ok(())
    })();
    drop(lock);
    let cleanup = fs::remove_file(&lock_path).map_err(|error| Error::io(&lock_path, error));
    result?;
    cleanup
}

enum MergeTree {
    Clean(String),
    Conflict,
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

pub(crate) fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Result<bool, Error> {
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
        Some(0) => clean_merge_tree_output(output),
        Some(1) => Ok(MergeTree::Conflict),
        _ if String::from_utf8_lossy(&output.stderr).contains("--write-tree") => {
            merged_tree_legacy(root, local, remote)
        }
        _ => Err(command_failed("prepare sync merge", output)),
    }
}

fn clean_merge_tree_output(output: Output) -> Result<MergeTree, Error> {
    let text = String::from_utf8(output.stdout)
        .map_err(|error| Error::InvalidSyncRemote(error.to_string()))?;
    let tree = text.lines().next().unwrap_or_default().trim();
    if tree.is_empty() {
        return Err(Error::InvalidSyncRemote(
            "Git merge produced no tree identifier".into(),
        ));
    }
    Ok(MergeTree::Clean(tree.to_owned()))
}

fn merged_tree_legacy(root: &Path, local: &str, remote: &str) -> Result<MergeTree, Error> {
    let temporary = tempfile::tempdir().map_err(|error| Error::io(root, error))?;
    let worktree = temporary.path().join("worktree");
    fs::create_dir(&worktree).map_err(|error| Error::io(&worktree, error))?;
    let index = temporary.path().join("index");

    let bases = required_git_text(
        root,
        "find sync merge base",
        &["merge-base", "--all", local, remote],
    )?;
    let bases: Vec<&str> = bases
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if bases.is_empty() {
        return Err(Error::InvalidSyncRemote(
            "Git produced no merge base for divergent sync history".into(),
        ));
    }

    let run_with_temporary_index = |args: &[&str]| -> Result<Output, Error> {
        Command::new("git")
            .current_dir(root)
            .arg("--git-dir=.git")
            .env("GIT_INDEX_FILE", &index)
            .env("GIT_WORK_TREE", &worktree)
            .args(args)
            .output()
            .map_err(|source| Error::GitUnavailable {
                path: root.to_path_buf(),
                source,
            })
    };

    let output = run_with_temporary_index(&["read-tree", "--reset", "-u", local])?;
    if !output.status.success() {
        return Err(command_failed("prepare legacy sync worktree", output));
    }

    let mut command = Command::new("git");
    command
        .current_dir(root)
        .arg("--git-dir=.git")
        .env("GIT_INDEX_FILE", &index)
        .env("GIT_WORK_TREE", &worktree)
        .arg("merge-recursive");
    for base in &bases {
        command.arg(base);
    }
    let output = command
        .arg("--")
        .arg(local)
        .arg(remote)
        .output()
        .map_err(|source| Error::GitUnavailable {
            path: root.to_path_buf(),
            source,
        })?;

    match output.status.code() {
        Some(0) => {
            let output = run_with_temporary_index(&["write-tree"])?;
            if !output.status.success() {
                return Err(command_failed("write legacy sync merge tree", output));
            }
            clean_merge_tree_output(output)
        }
        Some(1) => Ok(MergeTree::Conflict),
        _ => Err(command_failed("prepare legacy sync merge", output)),
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

pub(crate) fn optional_git_text(
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

    use super::{required_git_text, SyncOutcome};
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

    fn configure_tracking_origin(root: &Path, remote: &Path) {
        run_git(root, &["remote", "add", "origin", remote.to_str().unwrap()]);
        run_git(root, &["branch", "-M", "writing"]);
        run_git(root, &["config", "branch.writing.remote", "origin"]);
        run_git(
            root,
            &["config", "branch.writing.merge", "refs/heads/carta"],
        );
    }

    #[test]
    fn adopts_origin_tracking_and_publishes_local_checkpoint() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let source_path = temporary.path().join("source");
        let source = Archive::create(&source_path).unwrap();
        configure_tracking_origin(&source_path, &remote);
        run_git(
            &source_path,
            &["push", "--quiet", "origin", "HEAD:refs/heads/carta"],
        );
        run_git(&remote, &["symbolic-ref", "HEAD", "refs/heads/carta"]);
        let cloned_path = temporary.path().join("cloned");
        run_git(
            temporary.path(),
            &[
                "clone",
                "--quiet",
                remote.to_str().unwrap(),
                cloned_path.to_str().unwrap(),
            ],
        );
        drop(source);
        let mut archive = Archive::open(&cloned_path).unwrap();
        let before = required_git_text(&cloned_path, "test HEAD", &["rev-parse", "HEAD"]).unwrap();
        assert_eq!(archive.sync_remote().unwrap(), None);
        assert!(archive.enable_origin_sync_remote().unwrap());
        assert_eq!(archive.sync_remote().unwrap().as_deref(), remote.to_str());
        assert_eq!(
            required_git_text(&cloned_path, "test HEAD", &["rev-parse", "HEAD"]).unwrap(),
            before
        );
        assert!(!archive.is_dirty().unwrap());
        assert!(!archive.enable_origin_sync_remote().unwrap());
        archive.create_document("synthetic publication").unwrap();
        archive
            .checkpoint(CheckpointKind::Structural, Some("publication"))
            .unwrap();
        assert_eq!(archive.sync().unwrap().outcome(), SyncOutcome::Published);
        assert_eq!(
            required_git_text(
                &remote,
                "test remote HEAD",
                &["--git-dir=.", "rev-parse", "HEAD"],
            )
            .unwrap(),
            required_git_text(&cloned_path, "test local HEAD", &["rev-parse", "HEAD"]).unwrap(),
        );
    }

    #[test]
    fn adoption_preserves_origin_push_destinations() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let push_remote = bare_remote(&temporary.path().join("push"));
        let other_push_remote = bare_remote(&temporary.path().join("other-push"));
        let root = temporary.path().join("archive");
        let mut archive = Archive::create(&root).unwrap();
        configure_tracking_origin(&root, &remote);
        for destination in [&push_remote, &other_push_remote] {
            run_git(
                &root,
                &[
                    "config",
                    "--add",
                    "remote.origin.pushurl",
                    destination.to_str().unwrap(),
                ],
            );
        }
        assert!(archive.enable_origin_sync_remote().unwrap());
        assert_eq!(
            required_git_text(
                &root,
                "test push URLs",
                &["config", "--get-all", "remote.carta-sync.pushurl"]
            )
            .unwrap(),
            format!("{}\n{}", push_remote.display(), other_push_remote.display()),
        );
        assert_eq!(archive.sync().unwrap().outcome(), SyncOutcome::Published);
        for destination in [&push_remote, &other_push_remote] {
            assert_eq!(
                required_git_text(
                    destination,
                    "test published head",
                    &["--git-dir=.", "rev-parse", "refs/heads/carta"]
                )
                .unwrap(),
                required_git_text(&root, "test local HEAD", &["rev-parse", "HEAD"]).unwrap(),
            );
        }
        assert!(super::optional_git_text(
            &remote,
            "test fetch destination",
            &[
                "--git-dir=.",
                "rev-parse",
                "--verify",
                "--quiet",
                "refs/heads/carta",
            ]
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn origin_adoption_with_locked_config_is_unchanged_and_retryable() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let root = temporary.path().join("archive");
        let archive = Archive::create(&root).unwrap();
        configure_tracking_origin(&root, &remote);
        run_git(
            &root,
            &[
                "config",
                "remote.origin.pushurl",
                "synthetic-explicit-publication",
            ],
        );
        run_git(
            &root,
            &["config", "synthetic.concurrent-setting", "preserved"],
        );
        let config = root.join(".git/config");
        let before = std::fs::read(&config).unwrap();
        let lock = root.join(".git/config.lock");
        std::fs::write(&lock, "synthetic existing writer lock").unwrap();
        assert!(archive.enable_origin_sync_remote().is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(&lock).unwrap(),
            "synthetic existing writer lock"
        );
        assert_eq!(archive.sync_remote().unwrap(), None);
        std::fs::remove_file(&lock).unwrap();
        assert!(archive.enable_origin_sync_remote().unwrap());
        assert_eq!(
            required_git_text(
                &root,
                "test preserved concurrent setting",
                &["config", "--get", "synthetic.concurrent-setting"]
            )
            .unwrap(),
            "preserved"
        );
        assert_eq!(
            required_git_text(
                &root,
                "test adopted push destination",
                &["remote", "get-url", "--push", "--all", "carta-sync"]
            )
            .unwrap(),
            "synthetic-explicit-publication"
        );
        assert!(!lock.exists());
    }

    #[test]
    fn origin_adoption_preserves_all_default_urls_rewrites_and_includes() {
        for rewrite_mode in ["none", "instead-of", "push-instead-of"] {
            let temporary = tempfile::tempdir().unwrap();
            let remote = bare_remote(temporary.path());
            let other = bare_remote(&temporary.path().join("other"));
            let root = temporary.path().join("archive");
            let mut archive = Archive::create(&root).unwrap();
            configure_tracking_origin(&root, &remote);
            run_git(
                &root,
                &[
                    "config",
                    "--add",
                    "remote.origin.url",
                    other.to_str().unwrap(),
                ],
            );
            let included = root.join(".git/synthetic-settings");
            let content = "# untouched include\n[synthetic]\n\tunknown = retained\n";
            std::fs::write(&included, content).unwrap();
            run_git(&root, &["config", "include.path", "synthetic-settings"]);
            if rewrite_mode != "none" {
                run_git(
                    &root,
                    &[
                        "config",
                        "--replace-all",
                        "remote.origin.url",
                        "synthetic-fetch",
                    ],
                );
                run_git(
                    &root,
                    &["config", "--add", "remote.origin.url", "synthetic-other"],
                );
                for (url, prefix) in [(&remote, "synthetic-fetch"), (&other, "synthetic-other")] {
                    run_git(
                        &root,
                        &[
                            "config",
                            &format!("url.{}.insteadOf", url.display()),
                            prefix,
                        ],
                    );
                }
                if rewrite_mode == "push-instead-of" {
                    run_git(
                        &root,
                        &[
                            "config",
                            &format!("url.{}.pushInsteadOf", remote.display()),
                            "synthetic-fetch",
                        ],
                    );
                }
            }
            let fetch =
                required_git_text(&root, "test origin fetch", &["remote", "get-url", "origin"])
                    .unwrap();
            let pushes = required_git_text(
                &root,
                "test origin pushes",
                &["remote", "get-url", "--push", "--all", "origin"],
            )
            .unwrap();
            let raw = required_git_text(
                &root,
                "test raw origin URLs",
                &["config", "--get-all", "remote.origin.url"],
            )
            .unwrap();
            assert!(archive.enable_origin_sync_remote().unwrap());
            assert_eq!(
                required_git_text(
                    &root,
                    "test adopted raw URLs",
                    &["config", "--get-all", "remote.carta-sync.url"]
                )
                .unwrap(),
                raw
            );
            assert_eq!(
                required_git_text(
                    &root,
                    "test adopted fetch",
                    &["remote", "get-url", "carta-sync"]
                )
                .unwrap(),
                fetch
            );
            assert_eq!(
                required_git_text(
                    &root,
                    "test adopted pushes",
                    &["remote", "get-url", "--push", "--all", "carta-sync"]
                )
                .unwrap(),
                pushes
            );
            assert_eq!(std::fs::read_to_string(&included).unwrap(), content);
            assert_eq!(
                required_git_text(
                    &root,
                    "test preserved include",
                    &["config", "--get", "include.path"]
                )
                .unwrap(),
                "synthetic-settings"
            );
            assert_eq!(
                required_git_text(
                    &root,
                    "test included setting",
                    &["config", "--get", "synthetic.unknown"]
                )
                .unwrap(),
                "retained"
            );
            assert_eq!(archive.sync().unwrap().outcome(), SyncOutcome::Published);
            for destination in pushes.lines().map(Path::new) {
                assert_eq!(
                    required_git_text(
                        destination,
                        "test published checkpoint",
                        &["--git-dir=.", "rev-parse", "refs/heads/carta"]
                    )
                    .unwrap(),
                    required_git_text(&root, "test local checkpoint", &["rev-parse", "HEAD"])
                        .unwrap()
                );
            }
            assert!(!root.join(".git/config.lock").exists());
        }
    }

    #[test]
    fn malformed_origin_adoption_never_leaves_partial_config() {
        for malformed in [
            "empty-push",
            "multiline-push",
            "empty-second-url",
            "malformed-config",
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let remote = bare_remote(temporary.path());
            let root = temporary.path().join("archive");
            let archive = Archive::create(&root).unwrap();
            configure_tracking_origin(&root, &remote);
            let config = root.join(".git/config");
            match malformed {
                "empty-push" => run_git(&root, &["config", "remote.origin.pushurl", ""]),
                "multiline-push" => run_git(
                    &root,
                    &[
                        "config",
                        "remote.origin.pushurl",
                        "synthetic-one\nsynthetic-two",
                    ],
                ),
                "empty-second-url" => run_git(&root, &["config", "--add", "remote.origin.url", ""]),
                _ => {
                    let mut bytes = std::fs::read(&config).unwrap();
                    bytes.extend_from_slice(b"\n[malformed\n");
                    std::fs::write(&config, bytes).unwrap();
                }
            }
            let before = std::fs::read(&config).unwrap();
            assert!(archive.enable_origin_sync_remote().is_err(), "{malformed}");
            assert_eq!(std::fs::read(&config).unwrap(), before);
            assert!(!root.join(".git/config.lock").exists());
            if malformed != "malformed-config" {
                assert_eq!(archive.sync_remote().unwrap(), None);
                assert_eq!(
                    super::optional_git_text(
                        &root,
                        "test no partial enable preference",
                        &["config", "--local", "--get", "carta.sync-disabled"]
                    )
                    .unwrap(),
                    None
                );
            }
        }
    }

    #[test]
    fn dedicated_remote_takes_precedence_over_origin() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let dedicated = bare_remote(&temporary.path().join("dedicated"));
        let root = temporary.path().join("archive");
        let archive = Archive::create(&root).unwrap();
        configure_tracking_origin(&root, &remote);
        archive
            .set_sync_remote(dedicated.to_str().unwrap())
            .unwrap();
        run_git(
            &root,
            &[
                "config",
                "remote.carta-sync.pushurl",
                dedicated.to_str().unwrap(),
            ],
        );
        let config = std::fs::read(root.join(".git/config")).unwrap();
        assert!(!archive.enable_origin_sync_remote().unwrap());
        assert_eq!(std::fs::read(root.join(".git/config")).unwrap(), config);
    }

    #[test]
    fn explicit_remote_after_adoption_clears_all_old_push_urls() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        let root = temporary.path().join("archive");
        let archive = Archive::create(&root).unwrap();
        configure_tracking_origin(&root, &remote);
        for url in ["synthetic-old-one", "synthetic-old-two"] {
            run_git(&root, &["config", "--add", "remote.origin.pushurl", url]);
        }
        assert!(archive.enable_origin_sync_remote().unwrap());
        archive
            .set_sync_remote("synthetic-new-destination")
            .unwrap();
        assert_eq!(
            archive.sync_remote().unwrap().as_deref(),
            Some("synthetic-new-destination")
        );
        assert_eq!(
            super::optional_git_text(
                &root,
                "test cleared push URLs",
                &["config", "--get-all", "remote.carta-sync.pushurl"],
            )
            .unwrap(),
            None
        );
    }

    #[test]
    fn origin_adoption_requires_current_matching_upstream_and_url() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        for (index, args) in [
            vec!["config", "--unset", "branch.writing.remote"],
            vec!["config", "branch.writing.remote", "other"],
            vec!["config", "--unset", "branch.writing.merge"],
            vec!["config", "branch.writing.merge", "refs/heads/main"],
            vec!["config", "--unset", "remote.origin.url"],
            vec!["config", "remote.origin.url", ""],
            vec!["checkout", "--quiet", "--detach"],
            vec!["checkout", "--quiet", "-b", "untracked"],
        ]
        .iter()
        .enumerate()
        {
            let root = temporary.path().join(format!("archive-{index}"));
            let archive = Archive::create(&root).unwrap();
            configure_tracking_origin(&root, &remote);
            run_git(&root, args);
            let config = std::fs::read(root.join(".git/config")).unwrap();
            assert!(!archive.enable_origin_sync_remote().unwrap(), "{args:?}");
            assert_eq!(archive.sync_remote().unwrap(), None);
            assert_eq!(std::fs::read(root.join(".git/config")).unwrap(), config);
        }
    }

    #[test]
    fn explicit_disable_survives_reopen_and_explicit_reenable() {
        let temporary = tempfile::tempdir().unwrap();
        let remote = bare_remote(temporary.path());
        for initially_enabled in [false, true] {
            let root = temporary
                .path()
                .join(format!("archive-{initially_enabled}"));
            let archive = Archive::create(&root).unwrap();
            configure_tracking_origin(&root, &remote);
            if initially_enabled {
                assert!(archive.enable_origin_sync_remote().unwrap());
            }
            assert_eq!(archive.clear_sync_remote().unwrap(), initially_enabled);
            drop(archive);
            let archive = Archive::open(&root).unwrap();
            assert!(!archive.enable_origin_sync_remote().unwrap());
            assert_eq!(archive.sync_remote().unwrap(), None);
            assert_eq!(
                required_git_text(
                    &root,
                    "test disabled flag",
                    &["config", "--local", "--bool", "carta.sync-disabled"]
                )
                .unwrap(),
                "true"
            );
            archive.set_sync_remote(remote.to_str().unwrap()).unwrap();
            assert_eq!(
                required_git_text(
                    &root,
                    "test enabled flag",
                    &["config", "--local", "--bool", "carta.sync-disabled"]
                )
                .unwrap(),
                "false"
            );
            run_git(&root, &["config", "--remove-section", "remote.carta-sync"]);
            assert!(archive.enable_origin_sync_remote().unwrap());
        }
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
            "remote document\n"
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
    fn explicit_sync_configuration_updates_are_locked_and_retryable() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        archive.set_sync_remote("synthetic-original-fetch").unwrap();
        run_git(
            archive.root(),
            &[
                "config",
                "remote.carta-sync.pushurl",
                "synthetic-original-push",
            ],
        );
        run_git(
            archive.root(),
            &["config", "synthetic.unknown", "preserved"],
        );
        let config = archive.root().join(".git/config");
        let before = std::fs::read(&config).unwrap();
        let lock = archive.root().join(".git/config.lock");
        std::fs::write(&lock, "synthetic other writer").unwrap();
        assert!(archive.set_sync_remote("synthetic-new-fetch").is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert!(archive.clear_sync_remote().is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert!(archive.set_sync_remote("").is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(&lock).unwrap(),
            "synthetic other writer"
        );
        std::fs::remove_file(&lock).unwrap();
        archive.set_sync_remote("synthetic-new-fetch").unwrap();
        assert_eq!(
            archive.sync_remote().unwrap().as_deref(),
            Some("synthetic-new-fetch")
        );
        assert_eq!(
            super::optional_git_text(
                archive.root(),
                "test removed push URL",
                &["config", "--get-all", "remote.carta-sync.pushurl"]
            )
            .unwrap(),
            None
        );
        assert!(archive.clear_sync_remote().unwrap());
        assert_eq!(archive.sync_remote().unwrap(), None);
        assert_eq!(
            required_git_text(
                archive.root(),
                "test preserved setting",
                &["config", "--get", "synthetic.unknown"]
            )
            .unwrap(),
            "preserved"
        );
        assert!(!lock.exists());
    }

    #[test]
    fn failed_explicit_configuration_commands_do_not_publish_partial_changes() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        archive.set_sync_remote("synthetic-original-fetch").unwrap();
        run_git(
            archive.root(),
            &[
                "config",
                "remote.carta-sync.pushurl",
                "synthetic-original-push",
            ],
        );
        run_git(
            archive.root(),
            &["config", "--add", "carta.sync-disabled", "true"],
        );
        let config = archive.root().join(".git/config");
        let before = std::fs::read(&config).unwrap();
        // The final preference write fails after staging the URL/push changes.
        assert!(archive.set_sync_remote("synthetic-new-fetch").is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert!(!archive.root().join(".git/config.lock").exists());
    }

    #[test]
    fn inherited_routing_blocks_replacement_but_can_be_disabled() {
        for local in [false, true] {
            for routing in ["url", "pushurl", "both"] {
                let temporary = tempfile::tempdir().unwrap();
                let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
                if local {
                    archive.set_sync_remote("synthetic-local-fetch").unwrap();
                    run_git(
                        archive.root(),
                        &[
                            "config",
                            "remote.carta-sync.pushurl",
                            "synthetic-local-push",
                        ],
                    );
                }
                let include = archive.root().join(".git/synthetic-settings");
                let mut content = String::from("[synthetic]\n\tunknown = preserved\n[carta]\n\tsync-disabled = false\n[remote \"carta-sync\"]\n");
                if routing != "pushurl" {
                    content.push_str("\turl = synthetic-inherited-fetch\n");
                }
                if routing != "url" {
                    content.push_str("\tpushurl = synthetic-inherited-push\n");
                }
                std::fs::write(&include, &content).unwrap();
                run_git(
                    archive.root(),
                    &["config", "include.path", "synthetic-settings"],
                );
                let config = archive.root().join(".git/config");
                let before = std::fs::read(&config).unwrap();
                let enabled = archive.sync_remote().unwrap().is_some();
                let error = archive
                    .set_sync_remote("synthetic-requested-fetch")
                    .unwrap_err();
                assert!(matches!(error, Error::InvalidSyncRemote(_)), "{error:?}");
                assert_eq!(std::fs::read(&config).unwrap(), before);
                assert_eq!(std::fs::read_to_string(&include).unwrap(), content);
                assert!(!archive.root().join(".git/config.lock").exists());
                assert_eq!(archive.clear_sync_remote().unwrap(), enabled);
                assert_eq!(archive.sync_remote().unwrap(), None);
                assert_eq!(archive.sync().unwrap().outcome(), SyncOutcome::Disabled);
                assert!(crate::background_sync::stage_sync(archive.root().to_path_buf()).is_err());
                assert!(!archive.clear_sync_remote().unwrap());
                assert_eq!(std::fs::read_to_string(&include).unwrap(), content);
                assert_eq!(
                    required_git_text(
                        archive.root(),
                        "test preserved include",
                        &["config", "--get", "synthetic.unknown"]
                    )
                    .unwrap(),
                    "preserved"
                );
            }
        }
    }

    #[test]
    fn global_sync_routing_is_rejected_and_disable_is_authoritative() {
        // Isolate process-wide Git environment changes from parallel tests.
        if std::env::var_os("CARTA_TEST_SYNC_GLOBAL_CONFIG").is_none() {
            for conditional in [false, true] {
                let temporary = tempfile::tempdir().unwrap();
                let global = temporary.path().join("synthetic-global-config");
                let include = temporary.path().join("synthetic-conditional-settings");
                std::fs::write(
                    &include,
                    "[remote \"carta-sync\"]\n\tpushurl = synthetic-conditional-push\n",
                )
                .unwrap();
                if conditional {
                    std::fs::write(&global, "[carta]\n\tsync-disabled = false\n[includeIf \"hasconfig:remote.*.url:synthetic-requested-fetch\"]\n\tpath = synthetic-conditional-settings\n").unwrap();
                } else {
                    std::fs::write(&global, "[carta]\n\tsync-disabled = false\n[remote \"carta-sync\"]\n\turl = synthetic-global-fetch\n\tpushurl = synthetic-global-push\n").unwrap();
                }
                let before = std::fs::read(&global).unwrap();
                let status = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "sync::tests::global_sync_routing_is_rejected_and_disable_is_authoritative",
                    ])
                    .env(
                        "CARTA_TEST_SYNC_GLOBAL_CONFIG",
                        if conditional { "conditional" } else { "plain" },
                    )
                    .env("GIT_CONFIG_GLOBAL", &global)
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .status()
                    .unwrap();
                assert!(status.success());
                assert_eq!(std::fs::read(&global).unwrap(), before);
                assert_eq!(
                    std::fs::read_to_string(&include).unwrap(),
                    "[remote \"carta-sync\"]\n\tpushurl = synthetic-conditional-push\n"
                );
            }
            return;
        }
        let temporary = tempfile::tempdir().unwrap();
        let mut archive = Archive::create(temporary.path().join("archive")).unwrap();
        run_git(
            archive.root(),
            &[
                "config",
                "--local",
                "remote.carta-sync.url",
                "synthetic-local-fetch",
            ],
        );
        run_git(
            archive.root(),
            &[
                "config",
                "--local",
                "remote.carta-sync.pushurl",
                "synthetic-local-push",
            ],
        );
        let config = archive.root().join(".git/config");
        let before = std::fs::read(&config).unwrap();
        assert!(matches!(
            archive.set_sync_remote("synthetic-requested-fetch"),
            Err(Error::InvalidSyncRemote(_))
        ));
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert!(archive.clear_sync_remote().unwrap());
        assert_eq!(archive.sync_remote().unwrap(), None);
        assert_eq!(archive.sync().unwrap().outcome(), SyncOutcome::Disabled);
        assert!(crate::background_sync::stage_sync(archive.root().to_path_buf()).is_err());
        assert!(!archive.clear_sync_remote().unwrap());
        if std::env::var_os("CARTA_TEST_SYNC_GLOBAL_CONFIG").as_deref()
            == Some(std::ffi::OsStr::new("plain"))
        {
            assert_eq!(
                required_git_text(
                    archive.root(),
                    "test inherited raw URL remains",
                    &["config", "--get", "remote.carta-sync.url"]
                )
                .unwrap(),
                "synthetic-global-fetch"
            );
        }
    }

    #[test]
    fn stale_config_transaction_preserves_intervening_settings() {
        let temporary = tempfile::tempdir().unwrap();
        let archive = Archive::create(temporary.path().join("archive")).unwrap();
        let snapshot = super::sync_config_snapshot(archive.root()).unwrap();
        run_git(
            archive.root(),
            &["config", "synthetic.concurrent-setting", "newer value"],
        );
        let config = archive.root().join(".git/config");
        let before = std::fs::read(&config).unwrap();
        assert!(super::update_sync_config(
            archive.root(),
            &snapshot,
            None,
            None,
            &[vec!["carta.sync-disabled", "true"]]
        )
        .is_err());
        assert_eq!(std::fs::read(&config).unwrap(), before);
        assert!(!archive.root().join(".git/config.lock").exists());
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
            "from first\n"
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
            "first\n"
        );
        assert_eq!(
            second.read_document(second_document).unwrap().content(),
            "second\n"
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
            "second version\n"
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
