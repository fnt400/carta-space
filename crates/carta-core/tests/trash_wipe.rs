use std::fs;
use std::process::{Command, Output};

use carta_core::{Archive, CheckpointKind, Error};

fn create_archive() -> (tempfile::TempDir, Archive) {
    let temporary = tempfile::tempdir().unwrap();
    let archive = Archive::create(temporary.path().join("archive")).unwrap();
    (temporary, archive)
}

fn git(archive: &Archive, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(archive.root())
        .args(["--git-dir=.git", "--work-tree=."])
        .args(args)
        .output()
        .unwrap()
}

fn assert_git_ok(output: &Output) {
    assert!(
        output.status.success(),
        "Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn document_trash_reports_all_impacts_and_restore_uses_pre_trash_state() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("# Original\n\nbody").unwrap();
    let source = archive
        .create_document(&format!(
            "[first](carta:doc:{target}) and [second](carta:doc:{target})"
        ))
        .unwrap();
    let first_work = archive
        .create_work("First".to_owned(), vec![target, source])
        .unwrap();
    let second_work = archive
        .create_work("Second".to_owned(), vec![target])
        .unwrap();

    let impact = archive.document_trash_impact(target).unwrap();
    assert_eq!(
        impact
            .memberships()
            .iter()
            .map(|membership| (membership.id(), membership.title()))
            .collect::<Vec<_>>(),
        vec![(first_work, "First"), (second_work, "Second")]
    );
    assert_eq!(impact.inbound_links().len(), 2);
    assert!(impact
        .inbound_links()
        .iter()
        .all(|link| link.source() == source));

    let checkpoint = archive.trash_document(target).unwrap();
    assert_eq!(checkpoint.kind(), Some(CheckpointKind::Structural));
    assert!(matches!(
        archive.read_document(target),
        Err(Error::MissingDocument(id)) if id == target
    ));
    assert_eq!(archive.work(first_work).unwrap().documents(), &[source]);
    assert!(archive.work(second_work).unwrap().documents().is_empty());
    assert_eq!(
        archive.read_document(source).unwrap().content(),
        format!("[first](carta:doc:{target}) and [second](carta:doc:{target})")
    );
    assert!(!archive.is_dirty().unwrap());

    let trash = archive.trash_inventory().unwrap();
    assert_eq!(trash.documents().len(), 1);
    assert_eq!(trash.documents()[0].id(), target);
    assert_eq!(trash.documents()[0].label(), "Original");

    let wipe_plan = archive.plan_wipe_document(target).unwrap();
    archive.restore_trashed_document(target).unwrap();
    assert!(matches!(
        archive.execute_wipe_document(&wipe_plan, wipe_plan.confirmation_token()),
        Err(Error::DocumentIsActive(id)) if id == target
    ));
    assert_eq!(
        archive.read_document(target).unwrap().content(),
        "# Original\n\nbody"
    );
    assert!(archive.memberships(target).unwrap().is_empty());
    assert!(archive.trash_inventory().unwrap().documents().is_empty());
    assert!(!archive.is_dirty().unwrap());
}

#[test]
fn work_trash_restore_requires_document_consent_and_is_atomic() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("member").unwrap();
    let work = archive
        .create_work("Recoverable".to_owned(), vec![document])
        .unwrap();
    archive.trash_work(work).unwrap();
    archive.trash_document(document).unwrap();

    let before = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&before);
    let error = archive.restore_trashed_work(work, false, None).unwrap_err();
    assert!(matches!(
        error,
        Error::TrashedDocumentConsentRequired { documents, .. }
            if documents == vec![document]
    ));
    assert!(archive.work(work).is_none());
    assert!(matches!(
        archive.read_document(document),
        Err(Error::MissingDocument(id)) if id == document
    ));
    let after = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&after);
    assert_eq!(before.stdout, after.stdout);

    archive.restore_trashed_work(work, true, None).unwrap();
    assert_eq!(archive.work(work).unwrap().title(), "Recoverable");
    assert_eq!(archive.work(work).unwrap().documents(), &[document]);
    assert_eq!(archive.read_document(document).unwrap().content(), "member");
    assert!(!archive.is_dirty().unwrap());
}

#[test]
fn restoring_trashed_work_rejects_active_title_conflict_without_changes() {
    let (_temporary, mut archive) = create_archive();
    let work = archive.create_empty_work("Same title".to_owned()).unwrap();
    archive.trash_work(work).unwrap();
    let conflicting = archive
        .create_empty_work(" same title ".to_owned())
        .unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Conflicting Work"))
        .unwrap()
        .unwrap();

    let error = archive.restore_trashed_work(work, false, None).unwrap_err();
    assert!(matches!(
        error,
        Error::WorkTitleConflict { existing, .. } if existing == conflicting
    ));
    assert!(archive.work(work).is_none());
    assert_eq!(archive.works().count(), 1);
    assert!(!archive.is_dirty().unwrap());

    archive
        .restore_trashed_work(work, false, Some("Replacement".to_owned()))
        .unwrap();
    assert_eq!(archive.work(work).unwrap().title(), "Replacement");
    assert_eq!(archive.work(work).unwrap().id(), work);
}

#[test]
fn wipe_preserves_identical_content_owned_by_an_unrelated_document() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("identical body").unwrap();
    let survivor = archive.create_document("identical body").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Identical bodies"))
        .unwrap();
    archive.trash_document(target).unwrap();

    let plan = archive.plan_wipe_document(target).unwrap();
    archive
        .execute_wipe_document(&plan, plan.confirmation_token())
        .unwrap();

    assert_eq!(
        archive.read_document(survivor).unwrap().content(),
        "identical body"
    );
    let listing = git(&archive, &["rev-list", "--objects", "--all"]);
    assert_git_ok(&listing);
    assert!(!String::from_utf8(listing.stdout)
        .unwrap()
        .contains(&target.to_string()));
}

#[test]
fn wipe_rejects_orig_head_before_rewriting_refs() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("sensitive").unwrap();
    archive.trash_document(target).unwrap();
    let head = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&head);
    fs::write(archive.root().join(".git/ORIG_HEAD"), head.stdout).unwrap();

    assert!(matches!(
        archive.plan_wipe_document(target),
        Err(Error::WipeRepositoryState(state)) if state == "ORIG_HEAD"
    ));
}

#[test]
fn wipe_rejects_additional_recoverability_pseudoref_and_rebase_state() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("sensitive").unwrap();
    archive.trash_document(target).unwrap();
    let head = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&head);
    fs::write(archive.root().join(".git/AUTO_MERGE"), &head.stdout).unwrap();
    assert!(matches!(
        archive.plan_wipe_document(target),
        Err(Error::WipeRepositoryState(state)) if state == "AUTO_MERGE"
    ));
    fs::remove_file(archive.root().join(".git/AUTO_MERGE")).unwrap();

    fs::create_dir(archive.root().join(".git/rebase-merge")).unwrap();
    assert!(matches!(
        archive.plan_wipe_document(target),
        Err(Error::WipeRepositoryState(state)) if state == "rebase-merge"
    ));
}

#[test]
fn wipe_failure_after_ref_update_restores_original_refs() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("sensitive").unwrap();
    archive.trash_document(target).unwrap();
    let plan = archive.plan_wipe_document(target).unwrap();
    let before = git(&archive, &["show-ref"]);
    assert_git_ok(&before);

    std::env::set_var("CARTA_TEST_WIPE_FAIL_AFTER_REF_UPDATE", archive.root());
    let result = archive.execute_wipe_document(&plan, plan.confirmation_token());
    std::env::remove_var("CARTA_TEST_WIPE_FAIL_AFTER_REF_UPDATE");
    assert!(matches!(result, Err(Error::WipeVerificationFailed(_))));
    let after = git(&archive, &["show-ref"]);
    assert_git_ok(&after);
    assert_eq!(before.stdout, after.stdout);
    assert_eq!(
        archive.trash_inventory().unwrap().documents()[0].id(),
        target
    );
}

#[test]
fn wipe_rewrites_all_local_refs_prunes_objects_and_preserves_unrelated_state() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("secret version one").unwrap();
    let survivor = archive.create_document("surviving history one").unwrap();
    archive
        .checkpoint(CheckpointKind::Manual, Some("Both Documents"))
        .unwrap()
        .unwrap();
    archive.edit_document(target, "secret version two").unwrap();
    archive
        .edit_document(survivor, "surviving history two")
        .unwrap();
    archive
        .checkpoint(
            CheckpointKind::Manual,
            Some(&format!(
                "secret checkpoint for {target}: secret version two"
            )),
        )
        .unwrap()
        .unwrap();

    let target_path = archive
        .documents()
        .find(|info| info.id() == target)
        .unwrap()
        .path()
        .strip_prefix(archive.root())
        .unwrap()
        .join("content.md");
    let object = git(
        &archive,
        &[
            "rev-parse",
            &format!("HEAD:{}", target_path.to_string_lossy()),
        ],
    );
    assert_git_ok(&object);
    let content_object = String::from_utf8(object.stdout).unwrap().trim().to_owned();
    let branch = git(&archive, &["branch", "retained-copy", "HEAD"]);
    assert_git_ok(&branch);
    let tag = git(
        &archive,
        &[
            "-c",
            "user.name=Carta Test",
            "-c",
            "user.email=test@carta.space",
            "tag",
            "-a",
            "retained-tag",
            "-m",
            &format!("secret annotated tag for {target}: secret version two"),
            "HEAD",
        ],
    );
    assert_git_ok(&tag);

    assert!(matches!(
        archive.plan_wipe_document(target),
        Err(Error::DocumentIsActive(id)) if id == target
    ));
    archive.trash_document(target).unwrap();
    fs::create_dir(archive.root().join(".carta-cache")).unwrap();
    fs::write(
        archive.root().join(".carta-cache").join("target-copy"),
        "secret version two",
    )
    .unwrap();
    fs::write(archive.root().join("protected-untracked"), "keep").unwrap();

    let plan = archive.plan_wipe_document(target).unwrap();
    assert!(plan
        .retained_refs()
        .iter()
        .any(|name| name.ends_with("retained-copy")));
    assert!(matches!(
        archive.execute_wipe_document(&plan, "not the token"),
        Err(Error::WipeConfirmationMismatch)
    ));
    let report = archive
        .execute_wipe_document(&plan, plan.confirmation_token())
        .unwrap();
    assert!(report.rewritten_refs() >= 3);
    assert!(report.rewritten_commits() > 0);
    assert!(report.removed_artifacts() >= 1);
    assert_eq!(
        fs::read_to_string(archive.root().join("protected-untracked")).unwrap(),
        "keep"
    );
    assert!(!archive.root().join(".carta-cache").exists());
    assert_eq!(
        archive.read_document(survivor).unwrap().content(),
        "surviving history two"
    );
    assert_eq!(archive.document_revisions(survivor).unwrap().len(), 2);
    assert!(archive.trash_inventory().unwrap().documents().is_empty());
    let annotation = git(
        &archive,
        &["tag", "-l", "--format=%(contents:subject)", "retained-tag"],
    );
    assert_git_ok(&annotation);
    assert_eq!(
        String::from_utf8(annotation.stdout).unwrap().trim(),
        "Carta Space Wipe"
    );

    let paths = git(&archive, &["rev-list", "--objects", "--all"]);
    assert_git_ok(&paths);
    let listing = String::from_utf8(paths.stdout).unwrap();
    assert!(!listing.contains(&target.to_string()));
    assert!(!listing
        .lines()
        .any(|line| line.starts_with(&content_object)));
    for object in listing.lines().filter_map(|line| line.split(' ').next()) {
        let contents = git(&archive, &["cat-file", "-p", object]);
        assert_git_ok(&contents);
        let contents = String::from_utf8_lossy(&contents.stdout);
        assert!(!contents.contains(&target.to_string()));
        assert!(!contents.contains("secret version"));
        assert!(!contents.contains("secret checkpoint"));
        assert!(!contents.contains("secret annotated tag"));
    }
    assert!(!git(&archive, &["cat-file", "-e", &content_object])
        .status
        .success());
    assert_git_ok(&git(&archive, &["fsck", "--full", "--no-reflogs"]));
}

#[test]
fn stale_wipe_plan_refuses_execution_without_rewriting_history() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("sensitive").unwrap();
    archive.trash_document(target).unwrap();
    let plan = archive.plan_wipe_document(target).unwrap();
    let before = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&before);

    archive.create_document("later").unwrap();
    archive
        .checkpoint(CheckpointKind::Manual, Some("Later checkpoint"))
        .unwrap()
        .unwrap();
    assert!(matches!(
        archive.execute_wipe_document(&plan, plan.confirmation_token()),
        Err(Error::StaleWipePlan)
    ));
    assert!(!archive.trash_inventory().unwrap().documents().is_empty());
    let after = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&after);
    assert_ne!(before.stdout, after.stdout);
}

#[test]
fn wipe_removes_registered_stale_external_temporaries() {
    let (temporary, mut archive) = create_archive();
    let target = archive.create_document("sensitive temporary").unwrap();
    archive.trash_document(target).unwrap();
    let export_source = archive.create_document("export source").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Export source"))
        .unwrap();
    let destination = temporary.path().join("crashed.md");
    std::env::set_var("CARTA_TEST_EXPORT_CRASH_AFTER_TEMP_WRITE", archive.root());
    let result = archive.export_document_markdown_file(export_source, &destination);
    std::env::remove_var("CARTA_TEST_EXPORT_CRASH_AFTER_TEMP_WRITE");
    assert!(matches!(result, Err(Error::InvalidPackage(_))));
    let registry = archive.root().join(".git/carta-external-temporaries.json");
    let registered: Vec<String> = serde_json::from_slice(&fs::read(&registry).unwrap()).unwrap();
    let stale = std::path::PathBuf::from(&registered[0]);
    assert!(stale.exists());

    let plan = archive.plan_wipe_document(target).unwrap();
    archive
        .execute_wipe_document(&plan, plan.confirmation_token())
        .unwrap();
    assert!(!stale.exists());
    assert!(!registry.exists());
}

#[cfg(unix)]
#[test]
fn failed_trash_checkpoint_rolls_back_document_and_work_atomically() {
    use std::os::unix::fs::PermissionsExt;

    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("must remain").unwrap();
    let work = archive
        .create_work("Must remain".to_owned(), vec![document])
        .unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Before failed Trash"))
        .unwrap()
        .unwrap();
    let before = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&before);

    let hook = archive.root().join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    let mut permissions = fs::metadata(&hook).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hook, permissions).unwrap();

    assert!(matches!(
        archive.trash_document(document),
        Err(Error::GitCommandFailed { .. })
    ));
    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "must remain"
    );
    assert_eq!(archive.work(work).unwrap().documents(), &[document]);
    assert!(!archive.is_dirty().unwrap());
    let after = git(&archive, &["rev-parse", "HEAD"]);
    assert_git_ok(&after);
    assert_eq!(before.stdout, after.stdout);
    assert!(!fs::read_dir(archive.root().join(".git"))
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("carta-transaction-")));
}
