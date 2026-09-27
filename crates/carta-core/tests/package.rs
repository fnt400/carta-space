use std::fs;
use std::io::Read;
use std::process::Command;

use carta_core::{Archive, CheckpointKind, Error};
use zip::{CompressionMethod, ZipArchive};

fn create_archive() -> (tempfile::TempDir, Archive) {
    let temporary = tempfile::tempdir().unwrap();
    let archive = Archive::create(temporary.path().join("archive")).unwrap();
    (temporary, archive)
}

#[test]
fn package_is_ordered_valid_complete_and_checkpoints_current_state() {
    let (temporary, mut archive) = create_archive();
    let document = archive.create_document("current authored state").unwrap();
    archive
        .set_sync_remote("ssh://example.invalid/carta.git")
        .unwrap();
    archive
        .checkpoint(CheckpointKind::Manual, Some("older state"))
        .unwrap();
    archive
        .edit_document(document, "packaged current state")
        .unwrap();
    fs::write(archive.root().join("future-resource"), b"preserved").unwrap();
    fs::create_dir(archive.root().join(".carta-session-copy")).unwrap();
    fs::write(
        archive.root().join(".carta-session-copy/content"),
        b"transient",
    )
    .unwrap();

    let destination = temporary.path().join("portable.cat");
    let report = archive.package(&destination).unwrap();
    assert!(report.checkpoint_created());
    assert_eq!(
        archive.sync_remote().unwrap().as_deref(),
        Some("ssh://example.invalid/carta.git")
    );

    let mut zip = ZipArchive::new(fs::File::open(&destination).unwrap()).unwrap();
    let first = zip.by_index(0).unwrap();
    assert_eq!(first.name(), "mimetype");
    assert_eq!(first.compression(), CompressionMethod::Stored);
    drop(first);
    let names: Vec<_> = (0..zip.len())
        .map(|index| zip.by_index(index).unwrap().name().to_owned())
        .collect();
    assert!(names.iter().any(|name| name == "carta.json"));
    assert!(names.iter().any(|name| name.starts_with("volumes/")));
    assert!(names.iter().any(|name| name.starts_with("works/")));
    assert!(names.iter().any(|name| name.starts_with(".git/objects/")));
    assert!(names.iter().any(|name| name == "future-resource"));
    assert!(!names
        .iter()
        .any(|name| name.contains(".carta-session-copy")));

    let unpacked = tempfile::tempdir().unwrap();
    zip.extract(unpacked.path()).unwrap();
    Archive::validate(unpacked.path()).unwrap();
    let unpacked_archive = Archive::open(unpacked.path()).unwrap();
    assert_eq!(unpacked_archive.sync_remote().unwrap(), None);
    assert_eq!(
        unpacked_archive.read_document(document).unwrap().content(),
        "packaged current state"
    );
    assert!(!unpacked_archive.is_dirty().unwrap());
    assert!(unpacked_archive.history().unwrap().len() >= 3);
    assert_eq!(
        fs::read(unpacked.path().join("future-resource")).unwrap(),
        b"preserved"
    );
}

#[test]
fn package_replaces_regular_destination_but_rejects_archive_destinations() {
    let (temporary, mut archive) = create_archive();
    archive.create_document("body").unwrap();
    let destination = temporary.path().join("archive.cat");
    fs::write(&destination, b"old").unwrap();
    archive.package(&destination).unwrap();
    assert_ne!(fs::read(&destination).unwrap(), b"old");

    let inside = archive.root().join("unsafe.cat");
    assert!(matches!(
        archive.package(&inside),
        Err(Error::DestinationInsideArchive(path)) if path == inside
    ));
}

#[test]
fn markdown_file_exports_are_atomic_and_reject_archive_destinations() {
    let (temporary, mut archive) = create_archive();
    let first = archive.create_document("first\n").unwrap();
    let second = archive.create_document("second").unwrap();
    let work = archive
        .create_work("Ordered".to_owned(), vec![second, first])
        .unwrap();
    let document_destination = temporary.path().join("document.md");
    fs::write(&document_destination, "old").unwrap();

    archive
        .export_document_markdown_file(first, &document_destination)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&document_destination).unwrap(),
        "first\n"
    );
    let work_destination = temporary.path().join("work.md");
    archive
        .export_work_markdown_file(work, &work_destination)
        .unwrap();
    assert_eq!(
        fs::read_to_string(&work_destination).unwrap(),
        "secondfirst\n"
    );

    let inside = archive.root().join("unsafe.md");
    assert!(matches!(
        archive.export_document_markdown_file(first, &inside),
        Err(Error::DestinationInsideArchive(path)) if path == inside
    ));
}

#[test]
fn package_contains_readable_exact_mimetype() {
    let (temporary, archive) = create_archive();
    let destination = temporary.path().join("minimal.cat");
    archive.package(&destination).unwrap();
    let mut zip = ZipArchive::new(fs::File::open(destination).unwrap()).unwrap();
    let mut mimetype = Vec::new();
    zip.by_name("mimetype")
        .unwrap()
        .read_to_end(&mut mimetype)
        .unwrap();
    assert_eq!(mimetype, carta_format::MIMETYPE);

    let output = Command::new("git")
        .current_dir(archive.root())
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn startup_removes_durably_registered_external_temporary() {
    let (temporary, mut archive) = create_archive();
    let document = archive.create_document("sensitive temporary").unwrap();
    let destination = temporary.path().join("crashed.md");
    std::env::set_var("CARTA_TEST_EXPORT_CRASH_AFTER_TEMP_WRITE", archive.root());
    let result = archive.export_document_markdown_file(document, &destination);
    std::env::remove_var("CARTA_TEST_EXPORT_CRASH_AFTER_TEMP_WRITE");
    assert!(matches!(result, Err(Error::InvalidPackage(_))));
    let registry = archive.root().join(".git/carta-external-temporaries.json");
    let registered: Vec<String> = serde_json::from_slice(&fs::read(&registry).unwrap()).unwrap();
    assert_eq!(registered.len(), 1);
    let external_temporary = std::path::PathBuf::from(&registered[0]);
    assert!(external_temporary.exists());
    let root = archive.root().to_path_buf();
    drop(archive);

    Archive::open(root).unwrap();
    assert!(!external_temporary.exists());
    assert!(!registry.exists());
}
