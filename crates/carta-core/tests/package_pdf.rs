use std::fs;
use std::io::Read;
use std::process::Command;

use carta_core::{Archive, CheckpointKind, Error, PdfExportOptions};
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

#[cfg(unix)]
#[test]
fn pdf_exports_exact_document_and_ordered_work_markdown_atomically() {
    use std::os::unix::fs::PermissionsExt;

    let (temporary, mut archive) = create_archive();
    let first = archive.create_document("first\n").unwrap();
    let second = archive.create_document("second").unwrap();
    let work = archive
        .create_work("Ordered".to_owned(), vec![second, first])
        .unwrap();
    let capture = temporary.path().join("captured.md");
    let arguments = temporary.path().join("arguments.txt");
    let pandoc = temporary.path().join("pandoc-test");
    fs::write(
        &pandoc,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\noutput=''\nwhile [ \"$#\" -gt 0 ]; do\n  if [ \"$1\" = '--output' ]; then shift; output=$1; fi\n  shift\ndone\ncat > '{}'\nprintf 'PDF' > \"$output\"\n",
            arguments.display(),
            capture.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&pandoc).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&pandoc, permissions).unwrap();
    let options = PdfExportOptions::new(&pandoc, "lualatex");

    let document_pdf = temporary.path().join("document.pdf");
    archive
        .export_document_pdf_with(first, &document_pdf, &options)
        .unwrap();
    assert_eq!(fs::read_to_string(&capture).unwrap(), "first\n");
    assert!(fs::read_to_string(&arguments)
        .unwrap()
        .contains("--pdf-engine=lualatex"));
    assert_eq!(fs::read(&document_pdf).unwrap(), b"PDF");

    let work_pdf = temporary.path().join("work.pdf");
    archive
        .export_work_pdf_with(work, &work_pdf, &options)
        .unwrap();
    assert_eq!(fs::read_to_string(&capture).unwrap(), "secondfirst\n");
}

#[test]
fn pdf_reports_unavailable_pandoc_without_replacing_destination() {
    let (temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let destination = temporary.path().join("kept.pdf");
    fs::write(&destination, b"keep").unwrap();
    let options = PdfExportOptions::new(
        temporary.path().join("definitely-missing-pandoc"),
        "lualatex",
    );
    assert!(matches!(
        archive.export_document_pdf_with(document, &destination, &options),
        Err(Error::PandocUnavailable { .. })
    ));
    assert_eq!(fs::read(&destination).unwrap(), b"keep");
    assert!(!fs::read_dir(temporary.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".carta-pdf-")));
}

#[cfg(unix)]
#[test]
fn pdf_reports_renderer_failure_without_replacing_destination() {
    use std::os::unix::fs::PermissionsExt;

    let (temporary, mut archive) = create_archive();
    let document = archive.create_document("body").unwrap();
    let destination = temporary.path().join("kept.pdf");
    fs::write(&destination, b"keep").unwrap();
    let pandoc = temporary.path().join("pandoc-failure");
    fs::write(
        &pandoc,
        "#!/bin/sh\ncat >/dev/null\necho renderer failed >&2\nexit 9\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&pandoc).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&pandoc, permissions).unwrap();

    assert!(matches!(
        archive.export_document_pdf_with(
            document,
            &destination,
            &PdfExportOptions::new(&pandoc, "lualatex")
        ),
        Err(Error::PdfExportFailed { stderr, .. }) if stderr == "renderer failed"
    ));
    assert_eq!(fs::read(&destination).unwrap(), b"keep");
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
