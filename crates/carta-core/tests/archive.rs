use std::fs;
use std::process::Command;
use std::str::FromStr;

use carta_core::{Archive, DocumentId, Error, ValidationIssueKind, Volume, WorkProjectionItem};
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
        "first\nsecond\nthird"
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
        Err(Error::ExternalChange(path)) if path == content_path
    ));
    assert_eq!(fs::read_to_string(&content_path).unwrap(), "external");

    archive.refresh().unwrap();
    archive.edit_document(document, "accepted").unwrap();
    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "accepted"
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
        Err(Error::ExternalChange(path)) if path == work_path
    ));
    let persisted: serde_json::Value =
        serde_json::from_slice(&fs::read(&work_path).unwrap()).unwrap();
    assert_eq!(persisted["title"], "External");

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
        "same body"
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

    assert_eq!(archive.read_document(target).unwrap().content(), "");
    assert_eq!(
        archive.read_document(source).unwrap().content(),
        format!("caffè[Continue \\[here\\]](carta:doc:{target}) fine")
    );
    assert!(archive.memberships(target).unwrap().is_empty());
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
    assert_eq!(recovered.read_document(document).unwrap().content(), "body");
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
    assert_eq!(recovered.read_document(target).unwrap().content(), "");
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
