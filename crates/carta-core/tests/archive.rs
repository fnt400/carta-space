use std::fs;
use std::process::Command;
use std::str::FromStr;

use carta_core::{
    Archive, CheckpointKind, DocumentId, Error, ValidationIssueKind, Volume, WorkProjectionItem,
    WorkRestoreOptions,
};
use tempfile::TempDir;

fn create_archive() -> (TempDir, Archive) {
    let temporary = tempfile::tempdir().unwrap();
    let archive = Archive::create(temporary.path().join("archive")).unwrap();
    (temporary, archive)
}

#[test]
fn creates_opens_and_validates_a_minimal_archive() {
    let (_temporary, archive) = create_archive();

    assert_eq!(
        fs::read(archive.root().join("mimetype")).unwrap(),
        carta_format::MIMETYPE
    );
    assert!(archive.root().join("volumes").is_dir());
    assert!(archive.root().join("works").is_dir());
    assert!(archive.root().join(".git").is_dir());
    Archive::validate(archive.root()).unwrap();
    Archive::open(archive.root()).unwrap();

    let output = Command::new("git")
        .args([
            "-C",
            archive.root().to_str().unwrap(),
            "rev-parse",
            "--is-inside-work-tree",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "true");
}

#[test]
fn open_recovers_empty_structural_roots_omitted_by_git_clone() {
    let temporary = tempfile::tempdir().unwrap();
    let source = Archive::create(temporary.path().join("source")).unwrap();
    let clone = temporary.path().join("clone");

    let status = Command::new("git")
        .args(["clone", "--quiet"])
        .arg(source.root())
        .arg(&clone)
        .status()
        .unwrap();
    assert!(status.success());
    assert!(!clone.join("volumes").exists());
    assert!(!clone.join("works").exists());

    let cloned = Archive::open(&clone).unwrap();
    assert!(cloned.root().join("volumes").is_dir());
    assert!(cloned.root().join("works").is_dir());
    assert!(!cloned.is_dirty().unwrap());
}

#[test]
fn open_does_not_recreate_a_missing_root_with_tracked_content() {
    let (_temporary, mut archive) = create_archive();
    archive.create_document("tracked").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("tracked document"))
        .unwrap();
    let root = archive.root().to_path_buf();
    fs::remove_dir_all(root.join("volumes")).unwrap();
    drop(archive);

    assert!(matches!(
        Archive::open(&root),
        Err(Error::InvalidArchive(_))
    ));
    assert!(!root.join("volumes").exists());
}

#[test]
fn rejects_a_fake_git_directory() {
    let (_temporary, archive) = create_archive();
    let git_directory = archive.root().join(".git");
    fs::remove_dir_all(&git_directory).unwrap();
    fs::create_dir(&git_directory).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::InvalidGitWorkingTree(_))));
}

#[test]
fn creates_and_reads_documents_without_changing_content() {
    let (_temporary, mut archive) = create_archive();
    let content = "# Titolo\n\nAccenti: à è é ì ò ù.\n\n```\nraw <text>\n```\n";
    let id = archive.create_document(content).unwrap();
    let document = archive.read_document(id).unwrap();
    let info = archive.documents().next().unwrap();

    assert_eq!(document.content().as_bytes(), content.as_bytes());
    assert_eq!(document.metadata().id(), id);
    assert_eq!(info.id(), id);
    assert!(info.path().ends_with(id.to_string()));
    Archive::validate(archive.root()).unwrap();
}

#[test]
fn creates_works_in_normative_document_order() {
    let (_temporary, mut archive) = create_archive();
    let first = archive.create_document("first").unwrap();
    let second = archive.create_document("second").unwrap();
    let work_id = archive
        .create_work("Ordered work".to_owned(), vec![second, first])
        .unwrap();
    let work = archive.work(work_id).unwrap();

    assert_eq!(work.title(), "Ordered work");
    assert_eq!(work.documents(), &[second, first]);
    Archive::validate(archive.root()).unwrap();
}

#[test]
fn persists_optional_work_color_and_preserves_it_across_rename() {
    let (_temporary, mut archive) = create_archive();
    let work = archive.create_empty_work("Colored".to_owned()).unwrap();
    assert_eq!(archive.work(work).unwrap().color(), None);

    archive
        .set_work_color(work, Some("#6F7F8C".to_owned()))
        .unwrap();
    assert_eq!(archive.work(work).unwrap().color(), Some("#6F7F8C"));

    archive.rename_work(work, "Renamed".to_owned()).unwrap();
    assert_eq!(archive.work(work).unwrap().color(), Some("#6F7F8C"));

    let reopened = Archive::open(archive.root()).unwrap();
    assert_eq!(reopened.work(work).unwrap().color(), Some("#6F7F8C"));
}

#[test]
fn rejects_duplicate_documents_in_a_work() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();

    assert!(matches!(
        archive.create_work("Duplicate".to_owned(), vec![document, document]),
        Err(Error::DuplicateWorkDocument(duplicate)) if duplicate == document
    ));
}

#[test]
fn rejects_equivalent_active_work_titles_and_preserves_authored_title() {
    let (_temporary, mut archive) = create_archive();
    let title = "  Straße é  ";
    let work = archive.create_work(title.to_owned(), Vec::new()).unwrap();
    assert_eq!(archive.work(work).unwrap().title(), title);

    let decomposed = "STRASSE e\u{301}";
    assert!(matches!(
        archive.create_work(decomposed.to_owned(), Vec::new()),
        Err(Error::WorkTitleConflict { .. })
    ));
}

#[test]
fn normalizes_created_document_content_to_lf() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("first\r\nsecond\rthird").unwrap();

    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "first\nsecond\nthird\n"
    );
    Archive::validate(archive.root()).unwrap();
}

#[test]
fn rejects_noncanonical_external_line_endings() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(path, b"first\r\nsecond").unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::NonCanonicalLineEndings)));
}

#[test]
fn open_migrates_clean_legacy_document_to_trailing_lf() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Created Document"))
        .unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(&path, b"body").unwrap();
    archive
        .checkpoint(
            CheckpointKind::Structural,
            Some("Legacy Document without trailing LF"),
        )
        .unwrap();
    let history_before = archive.history().unwrap().len();
    let root = archive.root().to_path_buf();
    drop(archive);

    let reopened = Archive::open(&root).unwrap();

    assert_eq!(reopened.read_document(document).unwrap().content(), "body\n");
    assert!(!reopened.is_dirty().unwrap());
    assert_eq!(reopened.history().unwrap().len(), history_before + 1);
    Archive::validate(&root).unwrap();
}

#[test]
fn open_normalizes_dirty_legacy_document_without_committing_external_changes() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Created Document"))
        .unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(&path, b"external edit").unwrap();
    let root = archive.root().to_path_buf();
    drop(archive);

    let reopened = Archive::open(&root).unwrap();

    assert_eq!(
        reopened.read_document(document).unwrap().content(),
        "external edit\n"
    );
    assert!(reopened.is_dirty().unwrap());
    Archive::validate(&root).unwrap();
}

#[test]
fn rejects_document_without_canonical_trailing_lf() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(path, b"body").unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::NonCanonicalLineEndings)));
}

#[test]
fn rejects_a_new_work_with_a_missing_document() {
    let (_temporary, mut archive) = create_archive();
    let missing = DocumentId::from_str("01890f3e-70a9-7cc3-98c4-dc0c0c073990").unwrap();
    let error = archive
        .create_work("Broken".to_owned(), vec![missing])
        .unwrap_err();

    assert!(matches!(
        error,
        Error::DanglingDocumentReference { document, .. } if document == missing
    ));
    assert_eq!(archive.works().count(), 0);
}

#[test]
fn rejects_malformed_reserved_entries_but_tolerates_unknown_resources() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let document_path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .to_path_buf();

    fs::write(archive.root().join("future-root-resource"), b"preserve").unwrap();
    fs::write(document_path.join("future-resource"), b"preserve").unwrap();
    Archive::validate(archive.root()).unwrap();

    fs::write(archive.root().join("volumes").join("malformed"), b"bad").unwrap();
    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::MalformedReservedEntry(_))));
    assert_eq!(
        fs::read(archive.root().join("future-root-resource")).unwrap(),
        b"preserve"
    );
}

#[test]
fn rejects_non_utf8_document_content() {
    let (_temporary, mut archive) = create_archive();
    let id = archive.create_document("valid").unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == id)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(path, [0xff, 0xfe]).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::InvalidUtf8)));
}

#[test]
fn rejects_changed_mimetype_and_missing_required_files() {
    let (_temporary, mut archive) = create_archive();
    let id = archive.create_document("body").unwrap();
    fs::write(
        archive.root().join("mimetype"),
        b"application/vnd.carta-space+zip\n",
    )
    .unwrap();
    let metadata_path = archive
        .documents()
        .find(|info| info.id() == id)
        .unwrap()
        .path()
        .join("meta.json");
    fs::remove_file(metadata_path).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::InvalidMimetype)));
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::MissingEntry)));
}

#[test]
fn rejects_directory_and_metadata_id_mismatch() {
    let (_temporary, mut archive) = create_archive();
    let id = archive.create_document("body").unwrap();
    let original = archive
        .documents()
        .find(|info| info.id() == id)
        .unwrap()
        .path()
        .to_path_buf();
    let other = DocumentId::new_v7();
    let renamed = original.parent().unwrap().join(other.to_string());
    fs::rename(original, renamed).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors.issues().iter().any(|issue| matches!(
        issue.kind(),
        ValidationIssueKind::DirectoryIdMismatch { .. }
    )));
}

#[test]
fn rejects_noncanonical_uuid_directory_names() {
    let (_temporary, mut archive) = create_archive();
    let id = archive.create_document("body").unwrap();
    let original = archive
        .documents()
        .find(|info| info.id() == id)
        .unwrap()
        .path()
        .to_path_buf();
    let uppercase = original
        .parent()
        .unwrap()
        .join(id.to_string().to_uppercase());
    fs::rename(original, uppercase).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::MalformedReservedEntry(_))));
}

#[test]
fn rejects_document_in_the_wrong_volume() {
    let (_temporary, mut archive) = create_archive();
    let id = archive.create_document("body").unwrap();
    let info = archive.documents().find(|info| info.id() == id).unwrap();
    let original = info.path().to_path_buf();
    let wrong_month = if info.volume().month() == 12 {
        11
    } else {
        info.volume().month() + 1
    };
    let destination = archive
        .root()
        .join("volumes")
        .join(format!("{:04}", info.volume().year()))
        .join(format!("{wrong_month:02}"))
        .join(id.to_string());
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::rename(original, destination).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors
        .issues()
        .iter()
        .any(|issue| matches!(issue.kind(), ValidationIssueKind::VolumeMismatch { .. })));
}

#[test]
fn detects_dangling_work_references() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    archive
        .create_work("Work".to_owned(), vec![document])
        .unwrap();
    let document_path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .to_path_buf();
    fs::remove_dir_all(document_path).unwrap();

    let errors = Archive::validate(archive.root()).unwrap_err();
    assert!(errors.issues().iter().any(|issue| matches!(
        issue.kind(),
        ValidationIssueKind::DanglingDocumentReference { document: missing, .. }
            if *missing == document
    )));
}

#[test]
fn preserves_unknown_json_members_when_loading() {
    let (_temporary, archive) = create_archive();
    let path = archive.root().join("carta.json");
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    json["future"] = serde_json::json!({"answer": 42});
    fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

    let loaded = Archive::open(archive.root()).unwrap();
    assert_eq!(
        loaded.metadata().extensions()["future"],
        serde_json::json!({"answer": 42})
    );
}

#[test]
fn refuses_to_replace_an_existing_destination() {
    let temporary = tempfile::tempdir().unwrap();
    let destination = temporary.path().join("existing");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"unchanged").unwrap();

    assert!(matches!(
        Archive::create(&destination),
        Err(Error::AlreadyExists(path)) if path == destination
    ));
    assert_eq!(fs::read(destination.join("keep")).unwrap(), b"unchanged");
}

#[test]
fn detects_external_document_edits_and_refreshes_explicitly() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("loaded").unwrap();
    let content_path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(&content_path, "external").unwrap();

    assert!(matches!(
        archive.edit_document(document, "local"),
        Err(Error::ConflictPreserved(_))
    ));
    assert_eq!(fs::read_to_string(&content_path).unwrap(), "external");
    assert_eq!(archive.conflicts().unwrap().len(), 1);

    archive.refresh().unwrap();
    archive.edit_document(document, "accepted").unwrap();
    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "accepted\n"
    );
}

#[test]
fn detects_external_work_edits_without_overwriting_them() {
    let (_temporary, mut archive) = create_archive();
    let work = archive.create_empty_work("Loaded".to_owned()).unwrap();
    let work_path = archive.work(work).unwrap().path().join("work.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    json["title"] = serde_json::json!("External");
    fs::write(&work_path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

    assert!(matches!(
        archive.rename_work(work, "Local".to_owned()),
        Err(Error::ConflictPreserved(_))
    ));
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    assert_eq!(persisted["title"], "External");
    assert_eq!(archive.conflicts().unwrap().len(), 1);

    archive.reload().unwrap();
    archive.rename_work(work, "Accepted".to_owned()).unwrap();
    assert_eq!(archive.work(work).unwrap().title(), "Accepted");
}

#[test]
fn editing_preserves_document_identity_volume_metadata_and_unknown_resources() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("before").unwrap();
    let info = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap();
    let created = info.created();
    let volume = info.volume();
    let document_path = info.path().to_path_buf();
    let metadata_path = document_path.join("meta.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&metadata_path).unwrap()).unwrap();
    metadata["future"] = serde_json::json!({"kept": true});
    fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    fs::write(document_path.join("agent-resource"), "keep").unwrap();
    archive.refresh().unwrap();

    archive.edit_document(document, "after").unwrap();
    let info = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap();
    assert_eq!(info.created(), created);
    assert_eq!(info.volume(), volume);
    assert_eq!(info.metadata().extensions()["future"]["kept"], true);
    assert_eq!(
        fs::read_to_string(document_path.join("agent-resource")).unwrap(),
        "keep"
    );
}

#[test]
fn duplicates_as_a_neutral_document_with_new_identity() {
    let (_temporary, mut archive) = create_archive();
    let source = archive.create_document("same body").unwrap();
    let work = archive
        .create_work("Work".to_owned(), vec![source])
        .unwrap();
    let duplicate = archive.duplicate_document(source).unwrap();

    assert_ne!(duplicate, source);
    assert_eq!(
        archive.read_document(duplicate).unwrap().content(),
        "same body\n"
    );
    assert!(archive.memberships(duplicate).unwrap().is_empty());
    assert_eq!(archive.memberships(source).unwrap(), vec![work]);
}

#[test]
fn creates_linked_document_at_a_utf8_byte_boundary() {
    let (_temporary, mut archive) = create_archive();
    let source = archive.create_document("caffè fine").unwrap();
    let target = archive
        .new_linked_document(source, "caffè".len(), "Continue [here]")
        .unwrap();

    assert_eq!(archive.read_document(target).unwrap().content(), "\n");
    assert_eq!(
        archive.read_document(source).unwrap().content(),
        format!("caffè[Continue \\[here\\]](carta:doc:{target}) fine\n")
    );
    assert!(archive.memberships(target).unwrap().is_empty());
}

#[test]
fn splits_document_at_point_and_preserves_work_order() {
    let (_temporary, mut archive) = create_archive();
    let source = archive.create_document("alpha beta").unwrap();
    let trailing = archive.create_document("trailing").unwrap();
    let work = archive
        .create_work("Work".to_owned(), vec![source, trailing])
        .unwrap();
    let source_created = archive
        .documents()
        .find(|info| info.id() == source)
        .unwrap()
        .created();

    let target = archive.split_document_at(source, 6).unwrap();

    assert_eq!(archive.read_document(source).unwrap().content(), "alpha \n");
    assert_eq!(archive.read_document(target).unwrap().content(), "beta\n");
    let target_created = archive
        .documents()
        .find(|info| info.id() == target)
        .unwrap()
        .created();
    assert_eq!(target_created, source_created.successor());
    assert_eq!(
        archive.work(work).unwrap().documents(),
        &[source, target, trailing]
    );
}

#[test]
fn rejects_invalid_link_boundary_without_creating_a_document() {
    let (_temporary, mut archive) = create_archive();
    let source = archive.create_document("è").unwrap();
    let before = archive.documents().count();

    assert!(matches!(
        archive.new_linked_document(source, 1, "Continue"),
        Err(Error::InvalidByteBoundary { document, offset: 1 }) if document == source
    ));
    assert_eq!(archive.documents().count(), before);
}

#[test]
fn mutates_work_membership_order_and_title_without_losing_extensions() {
    let (_temporary, mut archive) = create_archive();
    let first = archive.create_document("first").unwrap();
    let second = archive.create_document("second").unwrap();
    let third = archive.create_document("third").unwrap();
    let work = archive.create_empty_work("Draft".to_owned()).unwrap();
    let work_path = archive.work(work).unwrap().path().join("work.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    json["future"] = serde_json::json!([1, 2, 3]);
    fs::write(&work_path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    archive.refresh().unwrap();

    archive.add_document_to_work(work, first).unwrap();
    archive.add_document_to_work(work, second).unwrap();
    archive.add_document_to_work(work, third).unwrap();
    assert!(archive.move_document_earlier(work, third).unwrap());
    assert!(archive.move_document_later(work, first).unwrap());
    archive.move_document_after(work, first, None).unwrap();
    archive.rename_work(work, "Final".to_owned()).unwrap();
    archive.remove_document_from_work(work, second).unwrap();

    let stored = archive.work(work).unwrap();
    assert_eq!(stored.title(), "Final");
    assert_eq!(stored.documents(), &[first, third]);
    assert_eq!(
        stored.metadata().extensions()["future"],
        serde_json::json!([1, 2, 3])
    );
    assert_eq!(archive.memberships(first).unwrap(), vec![work]);
    Archive::validate(archive.root()).unwrap();
}

#[test]
fn exposes_chronological_and_work_projections_without_content_copies() {
    let (_temporary, mut archive) = create_archive();
    let first = archive.create_document("first").unwrap();
    let second = archive.create_document("second").unwrap();
    let volume = archive
        .documents()
        .find(|info| info.id() == first)
        .unwrap()
        .volume();
    assert_eq!(archive.chronological_month(volume), vec![first, second]);
    assert!(archive
        .chronological_month(Volume::new(1900, 1).unwrap())
        .is_empty());

    let work = archive
        .create_work("Projection".to_owned(), vec![second, first])
        .unwrap();
    let projection = archive.work_projection(work).unwrap();
    assert_eq!(projection.work(), work);
    assert_eq!(
        projection.items(),
        &[
            WorkProjectionItem::Document(second),
            WorkProjectionItem::Boundary {
                before: second,
                after: first,
            },
            WorkProjectionItem::Document(first),
        ]
    );
}

#[test]
fn recovers_complete_unambiguous_document_staging_and_preserves_resources() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let destination = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .to_path_buf();
    fs::write(destination.join("future-resource"), "keep").unwrap();
    let staging = destination
        .parent()
        .unwrap()
        .join(format!(".carta-document-{document}"));
    fs::rename(&destination, &staging).unwrap();

    let recovered = Archive::open(archive.root()).unwrap();
    assert_eq!(recovered.read_document(document).unwrap().content(), "body\n");
    assert_eq!(
        fs::read_to_string(destination.join("future-resource")).unwrap(),
        "keep"
    );
    assert!(!staging.exists());
}

#[test]
fn rolls_forward_linked_staging_when_the_source_link_was_saved() {
    let (_temporary, mut archive) = create_archive();
    let source = archive.create_document("source").unwrap();
    let target = archive.create_document("").unwrap();
    let destination = archive
        .documents()
        .find(|info| info.id() == target)
        .unwrap()
        .path()
        .to_path_buf();
    let staging = destination
        .parent()
        .unwrap()
        .join(format!(".carta-linked-{target}"));
    fs::rename(&destination, &staging).unwrap();
    archive
        .edit_document(source, &format!("source[Continue](carta:doc:{target})"))
        .unwrap();

    let recovered = Archive::open(archive.root()).unwrap();
    assert_eq!(recovered.read_document(target).unwrap().content(), "\n");
    assert!(destination.is_dir());
    assert!(!staging.exists());
}

#[test]
fn rolls_back_abandoned_linked_staging_without_unknown_resources() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("").unwrap();
    let destination = archive
        .documents()
        .find(|info| info.id() == target)
        .unwrap()
        .path()
        .to_path_buf();
    let staging = destination
        .parent()
        .unwrap()
        .join(format!(".carta-linked-{target}"));
    fs::rename(&destination, &staging).unwrap();

    let recovered = Archive::open(archive.root()).unwrap();
    assert!(matches!(
        recovered.read_document(target),
        Err(Error::MissingDocument(id)) if id == target
    ));
    assert!(!destination.exists());
    assert!(!staging.exists());
}

#[test]
fn creation_makes_an_initial_checkpoint_with_carta_identity() {
    let (_temporary, archive) = create_archive();

    let history = archive.history().unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].kind(), Some(CheckpointKind::Structural));
    assert_eq!(history[0].note(), Some("Created Archive"));
    assert!(!archive.is_dirty().unwrap());

    let author = Command::new("git")
        .current_dir(archive.root())
        .args([
            "--git-dir=.git",
            "--work-tree=.",
            "show",
            "-s",
            "--format=%an <%ae>",
            "HEAD",
        ])
        .output()
        .unwrap();
    assert!(author.status.success());
    assert_eq!(
        String::from_utf8(author.stdout).unwrap().trim(),
        "Carta Space <history@carta.space>"
    );
}

#[test]
fn detects_dirty_state_and_records_typed_checkpoint_notes() {
    let (_temporary, mut archive) = create_archive();
    assert!(!archive.is_dirty().unwrap());

    let document = archive.create_document("first").unwrap();
    assert!(archive.is_dirty().unwrap());
    let first = archive
        .checkpoint(CheckpointKind::Automatic, None)
        .unwrap()
        .unwrap();
    assert_eq!(first.kind(), Some(CheckpointKind::Automatic));
    assert!(!archive.is_dirty().unwrap());
    assert!(archive
        .checkpoint(CheckpointKind::Manual, Some("unused"))
        .unwrap()
        .is_none());

    archive.edit_document(document, "second").unwrap();
    let second = archive
        .checkpoint(CheckpointKind::Manual, Some("Useful point"))
        .unwrap()
        .unwrap();
    assert_eq!(second.note(), Some("Useful point"));
    let history = archive.history().unwrap();
    assert_eq!(history[0].id(), second.id());
    assert_eq!(history[1].id(), first.id());

    fs::write(archive.root().join("untracked resource"), "dirty").unwrap();
    assert!(archive.is_dirty().unwrap());
}

#[test]
fn lists_reads_and_restores_document_revisions_without_rewriting_history() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("version one").unwrap();
    let first = archive
        .checkpoint(CheckpointKind::Manual, Some("First"))
        .unwrap()
        .unwrap();
    archive.edit_document(document, "version two").unwrap();
    let second = archive
        .checkpoint(CheckpointKind::Quit, None)
        .unwrap()
        .unwrap();

    let revisions = archive.document_revisions(document).unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].document().content(), "version two\n");
    assert_eq!(revisions[1].document().content(), "version one\n");
    assert_eq!(
        archive
            .read_document_revision(document, first.id())
            .unwrap()
            .content(),
        "version one\n"
    );

    let restored = archive
        .restore_document_version(document, first.id())
        .unwrap();
    assert_eq!(restored.kind(), Some(CheckpointKind::Structural));
    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "version one\n"
    );
    assert!(archive
        .history()
        .unwrap()
        .iter()
        .any(|checkpoint| checkpoint.id() == second.id()));

    let (new_document, _) = archive
        .restore_document_version_as_new(document, second.id())
        .unwrap();
    assert_ne!(new_document, document);
    assert_eq!(
        archive.read_document(new_document).unwrap().content(),
        "version two\n"
    );
    assert!(archive.memberships(new_document).unwrap().is_empty());
}

#[test]
fn historical_work_snapshot_and_restore_are_integral() {
    let (_temporary, mut archive) = create_archive();
    let first = archive.create_document("first old").unwrap();
    let second = archive.create_document("second old").unwrap();
    let work = archive
        .create_work("Old title".to_owned(), vec![second, first])
        .unwrap();
    let historical = archive
        .checkpoint(CheckpointKind::Structural, Some("Old Work"))
        .unwrap()
        .unwrap();

    let work_path = archive.work(work).unwrap().path().join("work.json");
    let mut metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    metadata["future"] = serde_json::json!({"generation": "current"});
    fs::write(&work_path, serde_json::to_vec_pretty(&metadata).unwrap()).unwrap();
    archive.refresh().unwrap();
    let current_created = archive.work(work).unwrap().created();

    archive.edit_document(first, "first new").unwrap();
    archive.edit_document(second, "second new").unwrap();
    archive.rename_work(work, "New title".to_owned()).unwrap();
    archive
        .move_document_after(work, second, Some(first))
        .unwrap();
    let later = archive
        .checkpoint(CheckpointKind::Manual, Some("Later Work"))
        .unwrap()
        .unwrap();

    let snapshot = archive.work_snapshot(work, historical.id()).unwrap();
    assert_eq!(snapshot.title(), "Old title");
    assert_eq!(snapshot.document_ids(), &[second, first]);
    assert_eq!(snapshot.documents()[0].content(), "second old\n");
    assert_eq!(snapshot.documents()[1].content(), "first old\n");

    archive
        .restore_work_version(work, historical.id(), WorkRestoreOptions::default())
        .unwrap();
    assert_eq!(archive.work(work).unwrap().title(), "Old title");
    assert_eq!(archive.work(work).unwrap().documents(), &[second, first]);
    assert_eq!(archive.work(work).unwrap().id(), work);
    assert_eq!(archive.work(work).unwrap().created(), current_created);
    assert_eq!(
        archive.work(work).unwrap().metadata().extensions()["future"],
        serde_json::json!({"generation": "current"})
    );
    assert_eq!(archive.read_document(first).unwrap().content(), "first old\n");
    assert_eq!(
        archive.read_document(second).unwrap().content(),
        "second old\n"
    );
    assert!(archive
        .history()
        .unwrap()
        .iter()
        .any(|checkpoint| checkpoint.id() == later.id()));
}

#[test]
fn work_restore_requires_consent_before_restoring_required_trashed_documents() {
    let (_temporary, mut archive) = create_archive();
    let kept = archive.create_document("kept").unwrap();
    let trashed = archive.create_document("restore me").unwrap();
    let work = archive
        .create_work("Work".to_owned(), vec![kept, trashed])
        .unwrap();
    let historical = archive
        .checkpoint(CheckpointKind::Structural, None)
        .unwrap()
        .unwrap();
    let trashed_path = archive
        .documents()
        .find(|document| document.id() == trashed)
        .unwrap()
        .path()
        .to_path_buf();

    archive.remove_document_from_work(work, trashed).unwrap();
    fs::remove_dir_all(&trashed_path).unwrap();
    archive.refresh().unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Simulated later Trash"))
        .unwrap()
        .unwrap();

    let error = archive
        .restore_work_version(work, historical.id(), WorkRestoreOptions::default())
        .unwrap_err();
    assert!(matches!(
        error,
        Error::TrashedDocumentConsentRequired { documents, .. }
            if documents == vec![trashed]
    ));
    assert!(!trashed_path.exists());
    assert_eq!(archive.work(work).unwrap().documents(), &[kept]);

    archive
        .restore_work_version(
            work,
            historical.id(),
            WorkRestoreOptions {
                restore_required_trashed_documents: true,
            },
        )
        .unwrap();
    assert_eq!(archive.work(work).unwrap().documents(), &[kept, trashed]);
    assert_eq!(
        archive.read_document(trashed).unwrap().content(),
        "restore me\n"
    );
}

#[test]
fn work_restore_reports_an_unrecoverable_required_document_without_changes() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let work = archive
        .create_work("Work".to_owned(), vec![document])
        .unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, None)
        .unwrap()
        .unwrap();
    let content_path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::remove_file(&content_path).unwrap();
    let broken = archive
        .checkpoint(CheckpointKind::Structural, Some("Broken external state"))
        .unwrap()
        .unwrap();

    let error = archive
        .restore_work_version(work, broken.id(), WorkRestoreOptions::default())
        .unwrap_err();
    assert!(matches!(
        error,
        Error::UnrecoverableWorkDocument { document: missing, .. } if missing == document
    ));
    assert!(!content_path.exists());
    assert!(!archive.is_dirty().unwrap());
}

#[test]
fn restore_refuses_to_mix_with_uncheckpointed_changes() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("old").unwrap();
    let checkpoint = archive
        .checkpoint(CheckpointKind::Manual, None)
        .unwrap()
        .unwrap();
    archive.edit_document(document, "uncheckpointed").unwrap();

    assert!(matches!(
        archive.restore_document_version(document, checkpoint.id()),
        Err(Error::RestoreRequiresCleanArchive)
    ));
    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "uncheckpointed\n"
    );
}

fn write_crash_manifest(archive: &Archive, cleanup: &[&str]) {
    let head = Command::new("git")
        .current_dir(archive.root())
        .args(["--git-dir=.git", "--work-tree=.", "rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(head.status.success());
    let owned = cleanup
        .iter()
        .map(|relative| {
            let path = archive.root().join(relative);
            let state = match fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => serde_json::json!({
                    "kind": "file",
                    "bytes": fs::read(path).unwrap(),
                }),
                Ok(metadata) if metadata.is_dir() => {
                    let mut entries = Vec::new();
                    snapshot_test_directory(&path, &path, &mut entries);
                    serde_json::json!({ "kind": "directory", "entries": entries })
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    serde_json::json!({ "kind": "missing" })
                }
                result => panic!("unexpected owned path state: {result:?}"),
            };
            serde_json::json!({ "path": relative, "state": state })
        })
        .collect::<Vec<_>>();
    fs::write(
        archive.root().join(".git/carta-transaction.json"),
        serde_json::to_vec(&serde_json::json!({
            "checkpoint": String::from_utf8(head.stdout).unwrap().trim(),
            "owned": owned,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn snapshot_test_directory(
    root: &std::path::Path,
    path: &std::path::Path,
    entries: &mut Vec<serde_json::Value>,
) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        let relative = path.strip_prefix(root).unwrap();
        if path.is_dir() {
            entries.push(serde_json::json!({ "path": relative, "directory": true }));
            snapshot_test_directory(root, &path, entries);
        } else {
            entries.push(serde_json::json!({
                "path": relative,
                "directory": false,
                "bytes": fs::read(path).unwrap(),
            }));
        }
    }
}

#[test]
fn reopen_rolls_back_interrupted_multi_object_trash_and_preserves_untracked_resources() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("must survive").unwrap();
    let work = archive
        .create_work("Work".to_owned(), vec![document])
        .unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Before crash"))
        .unwrap();
    let document_path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .to_path_buf();
    let transaction = format!(".git/carta-transaction-trash-document-{document}");
    let document_relative = document_path
        .strip_prefix(archive.root())
        .unwrap()
        .to_str()
        .unwrap();
    let work_path = archive.work(work).unwrap().path().join("work.json");
    let work_relative = work_path
        .strip_prefix(archive.root())
        .unwrap()
        .to_str()
        .unwrap();
    write_crash_manifest(&archive, &[&transaction, document_relative, work_relative]);
    fs::create_dir(archive.root().join(&transaction)).unwrap();
    archive.remove_document_from_work(work, document).unwrap();
    fs::rename(
        &document_path,
        archive.root().join(&transaction).join("document"),
    )
    .unwrap();
    fs::write(archive.root().join("protected-untracked"), "keep").unwrap();
    drop(archive);

    let recovered = Archive::open(document_path.ancestors().nth(4).unwrap()).unwrap();
    assert_eq!(
        recovered.read_document(document).unwrap().content(),
        "must survive\n"
    );
    assert_eq!(recovered.work(work).unwrap().documents(), &[document]);
    assert!(!recovered.root().join(transaction).exists());
    assert_eq!(
        fs::read_to_string(recovered.root().join("protected-untracked")).unwrap(),
        "keep"
    );
}

#[test]
fn reopen_rolls_back_interrupted_new_linked_document() {
    let (_temporary, mut archive) = create_archive();
    let source = archive.create_document("source").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Before linked crash"))
        .unwrap();
    let source_path = archive
        .documents()
        .find(|info| info.id() == source)
        .unwrap()
        .path()
        .join("content.md");
    let target = DocumentId::new_v7();
    let info = archive
        .documents()
        .find(|info| info.id() == source)
        .unwrap();
    let relative = format!(
        "volumes/{:04}/{:02}/{target}",
        info.volume().year(),
        info.volume().month()
    );
    let created = info.created();
    let source_relative = source_path
        .strip_prefix(archive.root())
        .unwrap()
        .to_str()
        .unwrap();
    write_crash_manifest(&archive, &[&relative, source_relative]);
    let target_path = archive.root().join(&relative);
    fs::create_dir(&target_path).unwrap();
    fs::write(
        target_path.join("meta.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"id": target, "created": created})).unwrap(),
    )
    .unwrap();
    fs::write(target_path.join("content.md"), "").unwrap();
    fs::write(
        &source_path,
        format!("source[Continue](carta:doc:{target})"),
    )
    .unwrap();
    let root = archive.root().to_path_buf();
    drop(archive);

    let recovered = Archive::open(&root).unwrap();
    assert_eq!(recovered.read_document(source).unwrap().content(), "source\n");
    assert!(!target_path.exists());
}

#[test]
fn interrupted_recovery_preserves_post_crash_external_tracked_edits() {
    let (_temporary, mut archive) = create_archive();
    let owned = archive.create_document("owned before").unwrap();
    let external = archive.create_document("external before").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Before simulated crash"))
        .unwrap();
    let owned_path = archive
        .documents()
        .find(|info| info.id() == owned)
        .unwrap()
        .path()
        .join("content.md");
    let external_path = archive
        .documents()
        .find(|info| info.id() == external)
        .unwrap()
        .path()
        .join("content.md");
    let owned_relative = owned_path
        .strip_prefix(archive.root())
        .unwrap()
        .to_str()
        .unwrap();
    write_crash_manifest(&archive, &[owned_relative]);
    fs::write(&owned_path, "transaction partial").unwrap();
    fs::write(&external_path, "external after crash").unwrap();
    let root = archive.root().to_path_buf();
    drop(archive);

    assert!(matches!(
        Archive::open(&root),
        Err(Error::AmbiguousTransactionRecovery(paths))
            if paths.iter().any(|path| root.join(path) == external_path)
    ));
    assert_eq!(
        fs::read_to_string(&owned_path).unwrap(),
        "transaction partial"
    );
    assert_eq!(
        fs::read_to_string(&external_path).unwrap(),
        "external after crash"
    );
    assert!(root.join(".git/carta-transaction.json").exists());
}

#[test]
fn interrupted_recovery_preserves_changed_owned_path_as_conflict() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("before crash").unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Before simulated crash"))
        .unwrap();
    let content_path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    let relative = content_path
        .strip_prefix(archive.root())
        .unwrap()
        .to_str()
        .unwrap();
    write_crash_manifest(&archive, &[relative]);
    fs::write(&content_path, "changed after crash").unwrap();
    let root = archive.root().to_path_buf();
    drop(archive);

    let recovered = Archive::open(&root).unwrap();
    assert_eq!(
        recovered.read_document(document).unwrap().content(),
        "before crash\n"
    );
    let conflicts = recovered.conflicts().unwrap();
    assert_eq!(conflicts.len(), 1);
    let carta_core::Conflict::Document(conflict) = &conflicts[0] else {
        panic!("expected a Document conflict");
    };
    assert_eq!(conflict.document(), document);
    assert_eq!(conflict.local(), b"changed after crash");
    assert_eq!(conflict.external(), b"before crash\n");
}

#[test]
fn interrupted_recovery_preserves_changed_operation_created_document() {
    let (_temporary, archive) = create_archive();
    let reference = archive.documents().next().map(|info| info.created());
    let created = reference.unwrap_or_else(carta_core::Timestamp::now_local);
    let volume = carta_core::Volume::new(created.year() as u16, created.month() as u8).unwrap();
    let document = DocumentId::new_v7();
    let relative = format!(
        "volumes/{:04}/{:02}/{document}",
        volume.year(),
        volume.month()
    );
    write_crash_manifest(&archive, &[&relative]);
    let directory = archive.root().join(&relative);
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("meta.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "id": document,
            "created": created,
        }))
        .unwrap(),
    )
    .unwrap();
    fs::write(directory.join("content.md"), "created after crash").unwrap();
    let root = archive.root().to_path_buf();
    drop(archive);

    let mut recovered = Archive::open(&root).unwrap();
    assert!(!directory.exists());
    let conflicts = recovered.conflicts().unwrap();
    let carta_core::Conflict::Document(conflict) = &conflicts[0] else {
        panic!("expected a Document conflict");
    };
    assert_eq!(conflict.document(), document);
    assert!(conflict.external_missing());
    assert_eq!(conflict.local(), b"created after crash");

    recovered
        .resolve_document_conflict(conflict.id(), carta_core::ConflictChoice::Local, false)
        .unwrap();
    assert_eq!(
        recovered.read_document(document).unwrap().content(),
        "created after crash\n"
    );
}

#[test]
fn reopen_removes_partial_restore_or_new_work_document_owned_paths() {
    let (_temporary, mut archive) = create_archive();
    let member = archive.create_document("member").unwrap();
    let work = archive
        .create_work("Work".to_owned(), vec![member])
        .unwrap();
    archive
        .checkpoint(CheckpointKind::Structural, Some("Before provisional crash"))
        .unwrap();
    let target = DocumentId::new_v7();
    let info = archive
        .documents()
        .find(|info| info.id() == member)
        .unwrap();
    let relative = format!(
        "volumes/{:04}/{:02}/{target}",
        info.volume().year(),
        info.volume().month()
    );
    let restore_relative = format!(
        "volumes/{:04}/{:02}/.carta-restore-{target}",
        info.volume().year(),
        info.volume().month()
    );
    let work_path = archive.work(work).unwrap().path().join("work.json");
    let work_relative = work_path
        .strip_prefix(archive.root())
        .unwrap()
        .to_str()
        .unwrap();
    write_crash_manifest(&archive, &[&relative, &restore_relative, work_relative]);
    let target_path = archive.root().join(&relative);
    fs::create_dir(&target_path).unwrap();
    fs::write(target_path.join("content.md"), "partial").unwrap();
    let restore_staging = target_path
        .parent()
        .unwrap()
        .join(format!(".carta-restore-{target}"));
    fs::create_dir(&restore_staging).unwrap();
    fs::write(restore_staging.join("content.md"), "staged").unwrap();
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    json["documents"] = serde_json::json!([member, target]);
    fs::write(&work_path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    let root = archive.root().to_path_buf();
    drop(archive);

    let recovered = Archive::open(&root).unwrap();
    assert_eq!(recovered.work(work).unwrap().documents(), &[member]);
    assert!(!target_path.exists());
    assert!(!restore_staging.exists());
}

#[test]
fn document_lock_persists_and_blocks_content_changes() {
    let (_temporary, mut archive) = create_archive();
    let document = archive.create_document("original").unwrap();

    archive.set_document_locked(document, true).unwrap();
    assert!(archive.document_is_explicitly_locked(document).unwrap());
    assert!(archive.document_is_locked(document).unwrap());
    assert!(matches!(
        archive.edit_document(document, "changed"),
        Err(Error::DocumentLocked(id)) if id == document
    ));

    let mut reopened = Archive::open(archive.root()).unwrap();
    assert!(reopened.document_is_explicitly_locked(document).unwrap());
    assert_eq!(
        reopened.read_document(document).unwrap().content(),
        "original\n"
    );

    reopened.set_document_locked(document, false).unwrap();
    reopened.edit_document(document, "changed").unwrap();
    assert!(!reopened.document_is_locked(document).unwrap());
    assert_eq!(
        reopened.read_document(document).unwrap().content(),
        "changed\n"
    );
}

#[test]
fn work_lock_makes_member_documents_effectively_read_only() {
    let (_temporary, mut archive) = create_archive();
    let first = archive.create_document("first").unwrap();
    let second = archive.create_document("second").unwrap();
    let work = archive
        .create_work("Locked work".to_owned(), vec![first])
        .unwrap();

    archive.set_work_locked(work, true).unwrap();
    assert!(archive.work_is_locked(work).unwrap());
    assert!(archive.document_is_locked(first).unwrap());
    assert!(!archive.document_is_locked(second).unwrap());
    assert!(matches!(
        archive.edit_document(first, "changed"),
        Err(Error::DocumentLocked(id)) if id == first
    ));
    assert!(matches!(
        archive.rename_work(work, "Renamed".to_owned()),
        Err(Error::WorkLocked(id)) if id == work
    ));
    assert!(matches!(
        archive.add_document_to_work(work, second),
        Err(Error::WorkLocked(id)) if id == work
    ));
    assert!(matches!(
        archive.set_work_color(work, Some("#667A75".to_owned())),
        Err(Error::WorkLocked(id)) if id == work
    ));

    archive.set_document_locked(first, true).unwrap();
    archive.set_work_locked(work, false).unwrap();
    assert!(!archive.work_is_locked(work).unwrap());
    assert!(archive.document_is_locked(first).unwrap());

    archive.set_document_locked(first, false).unwrap();
    archive.edit_document(first, "changed").unwrap();
    assert_eq!(archive.read_document(first).unwrap().content(), "changed\n");
}

#[test]
fn unchanged_locked_document_does_not_block_other_document_saves() {
    let (_temporary, mut archive) = create_archive();
    let locked = archive.create_document("locked").unwrap();
    let writable = archive.create_document("before").unwrap();
    archive.set_document_locked(locked, true).unwrap();

    archive.edit_document(locked, "locked").unwrap();
    archive.edit_document(writable, "after").unwrap();

    assert_eq!(archive.read_document(locked).unwrap().content(), "locked\n");
    assert_eq!(archive.read_document(writable).unwrap().content(), "after\n");
    assert!(matches!(
        archive.edit_document(locked, "changed"),
        Err(Error::DocumentLocked(id)) if id == locked
    ));
}
