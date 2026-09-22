use std::fs;
use std::process::Command;
use std::str::FromStr;

use carta_core::{Archive, DocumentId, Error, ValidationIssueKind};
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
        .create_work("Ordered work".to_owned(), vec![second, first, second])
        .unwrap();
    let work = archive.work(work_id).unwrap();

    assert_eq!(work.title(), "Ordered work");
    assert_eq!(work.documents(), &[second, first, second]);
    Archive::validate(archive.root()).unwrap();
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
