use std::fs;

use carta_core::{Archive, Conflict, ConflictChoice, Error};

#[test]
fn document_divergence_preserves_both_variants_and_resolves_explicitly() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut archive = Archive::create(&root).unwrap();
    let document = archive.create_document("base").unwrap();
    archive
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(&path, "external").unwrap();

    let conflict_id = match archive.edit_document(document, "local").unwrap_err() {
        Error::ConflictPreserved(id) => id,
        error => panic!("unexpected error: {error}"),
    };
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    let conflicts = archive.conflicts().unwrap();
    let Conflict::Document(conflict) = &conflicts[0] else {
        panic!("expected Document conflict")
    };
    assert_eq!(conflict.local(), b"local");
    assert_eq!(conflict.external(), b"external");
    let package = temporary.path().join("conflicted.cat");
    archive.package(&package).unwrap();
    let mut zip = zip::ZipArchive::new(fs::File::open(package).unwrap()).unwrap();
    assert!((0..zip.len()).all(|index| {
        !zip.by_index(index)
            .unwrap()
            .name()
            .contains("carta-conflicts")
    }));
    drop(zip);
    drop(archive);
    let mut archive = Archive::open(&root).unwrap();
    assert_eq!(archive.conflicts().unwrap().len(), 1);

    let preserved = archive
        .resolve_document_conflict(&conflict_id, ConflictChoice::Local, true)
        .unwrap()
        .unwrap();
    assert_eq!(archive.read_document(document).unwrap().content(), "local");
    assert_eq!(
        archive.read_document(preserved).unwrap().content(),
        "external"
    );
    assert!(archive.conflicts().unwrap().is_empty());
}

#[test]
fn work_divergence_preserves_intended_and_external_structures_without_second_work() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut local = Archive::create(&root).unwrap();
    let first = local.create_document("first").unwrap();
    let second = local.create_document("second").unwrap();
    let work = local.create_work("Original".into(), vec![first]).unwrap();
    local
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let mut external = Archive::open(&root).unwrap();
    external.rename_work(work, "External".into()).unwrap();

    let conflict_id = match local.add_document_to_work(work, second).unwrap_err() {
        Error::ConflictPreserved(id) => id,
        error => panic!("unexpected error: {error}"),
    };
    assert_eq!(local.conflicts().unwrap().len(), 1);
    local
        .resolve_work_conflict(&conflict_id, ConflictChoice::Local)
        .unwrap();
    assert_eq!(local.works().count(), 1);
    let resolved = local.work(work).unwrap();
    assert_eq!(resolved.title(), "Original");
    assert_eq!(resolved.documents(), &[first, second]);
}

#[test]
fn wipe_scrubs_recovery_variants_for_the_target_document() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut archive = Archive::create(&root).unwrap();
    let document = archive.create_document("base").unwrap();
    archive
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let path = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(&path, "external unique content").unwrap();
    assert!(matches!(
        archive.edit_document(document, "local sensitive variant"),
        Err(Error::ConflictPreserved(_))
    ));
    assert_eq!(archive.conflicts().unwrap().len(), 1);
    archive.refresh().unwrap();
    archive.trash_document(document).unwrap();

    let plan = archive.plan_wipe_document(document).unwrap();
    let confirmation = plan.confirmation_token().to_owned();
    archive.execute_wipe_document(&plan, &confirmation).unwrap();
    assert!(archive.conflicts().unwrap().is_empty());
    let conflict_directory = root.join(".git/carta-conflicts");
    assert!(conflict_directory.read_dir().unwrap().next().is_none());
}
