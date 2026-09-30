use std::fs;
use std::process::Command;

use carta_core::{Archive, Conflict, ConflictChoice, DocumentId, Error};

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
    assert_eq!(conflict.local(), b"local\n");
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
    assert_eq!(archive.read_document(document).unwrap().content(), "local\n");
    assert_eq!(
        archive.read_document(preserved).unwrap().content(),
        "external\n"
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
fn externally_missing_document_preserves_local_and_last_loaded_variants() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut archive = Archive::create(&root).unwrap();
    let document = archive.create_document("last loaded").unwrap();
    let work = archive
        .create_work("Still references missing".into(), vec![document])
        .unwrap();
    archive
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let directory = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .to_path_buf();
    fs::remove_dir_all(&directory).unwrap();

    let conflict_id = match archive
        .edit_document(document, "local intended")
        .unwrap_err()
    {
        Error::ConflictPreserved(id) => id,
        error => panic!("unexpected error: {error}"),
    };
    let Conflict::Document(conflict) = archive.conflicts().unwrap().remove(0) else {
        panic!("expected Document conflict")
    };
    assert!(conflict.external_missing());
    assert_eq!(conflict.external(), b"last loaded\n");
    drop(archive);

    let mut archive = Archive::open(&root).unwrap();
    assert_eq!(archive.work(work).unwrap().documents(), &[document]);
    let preserved = archive
        .resolve_document_conflict(&conflict_id, ConflictChoice::Local, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        archive.read_document(document).unwrap().content(),
        "local intended\n"
    );
    assert_eq!(
        archive.read_document(preserved).unwrap().content(),
        "last loaded\n"
    );
}

#[test]
fn externally_missing_work_preserves_and_restores_intended_structure() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut archive = Archive::create(&root).unwrap();
    let first = archive.create_document("first").unwrap();
    let second = archive.create_document("second").unwrap();
    let work = archive.create_work("Work".into(), vec![first]).unwrap();
    archive
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let directory = archive.work(work).unwrap().path().to_path_buf();
    fs::remove_dir_all(&directory).unwrap();

    let conflict_id = match archive.add_document_to_work(work, second).unwrap_err() {
        Error::ConflictPreserved(id) => id,
        error => panic!("unexpected error: {error}"),
    };
    let Conflict::Work(conflict) = archive.conflicts().unwrap().remove(0) else {
        panic!("expected Work conflict")
    };
    assert!(conflict.external_missing());
    drop(archive);

    let mut archive = Archive::open(&root).unwrap();
    archive
        .resolve_work_conflict(&conflict_id, ConflictChoice::Local)
        .unwrap();
    assert_eq!(archive.works().count(), 1);
    assert_eq!(archive.work(work).unwrap().documents(), &[first, second]);
}

#[test]
fn interrupted_document_conflict_resolution_restores_both_variants() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut archive = Archive::create(&root).unwrap();
    let document = archive.create_document("base").unwrap();
    archive
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let canonical = archive
        .documents()
        .find(|info| info.id() == document)
        .unwrap()
        .path()
        .join("content.md");
    fs::write(&canonical, "external").unwrap();
    let conflict_id = match archive.edit_document(document, "local").unwrap_err() {
        Error::ConflictPreserved(id) => id,
        error => panic!("unexpected error: {error}"),
    };
    archive
        .checkpoint(
            carta_core::CheckpointKind::Structural,
            Some("External state"),
        )
        .unwrap();
    let conflict = root.join(format!(".git/carta-conflicts/{conflict_id}.json"));
    let preserved = canonical
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(DocumentId::new_v7().to_string());
    write_file_transaction_manifest(&root, &[&canonical, &conflict, &preserved]);
    fs::write(&canonical, "local").unwrap();
    fs::create_dir(&preserved).unwrap();
    fs::write(preserved.join("content.md"), "external").unwrap();
    fs::remove_file(&conflict).unwrap();
    drop(archive);

    let recovered = Archive::open(&root).unwrap();
    assert_eq!(fs::read_to_string(canonical).unwrap(), "external");
    assert!(!preserved.exists());
    assert_eq!(recovered.conflicts().unwrap().len(), 1);
}

#[test]
fn interrupted_work_conflict_resolution_restores_both_structures() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("archive");
    let mut local = Archive::create(&root).unwrap();
    let first = local.create_document("first").unwrap();
    let second = local.create_document("second").unwrap();
    let work = local.create_work("Original".into(), vec![first]).unwrap();
    local
        .checkpoint(carta_core::CheckpointKind::Structural, None)
        .unwrap();
    let canonical = local.work(work).unwrap().path().join("work.json");
    let mut external = Archive::open(&root).unwrap();
    external.rename_work(work, "External".into()).unwrap();
    let conflict_id = match local.add_document_to_work(work, second).unwrap_err() {
        Error::ConflictPreserved(id) => id,
        error => panic!("unexpected error: {error}"),
    };
    local
        .checkpoint(
            carta_core::CheckpointKind::Structural,
            Some("External Work state"),
        )
        .unwrap();
    let conflict_path = root.join(format!(".git/carta-conflicts/{conflict_id}.json"));
    let Conflict::Work(conflict) = local.conflicts().unwrap().remove(0) else {
        panic!("expected Work conflict")
    };
    write_file_transaction_manifest(&root, &[&canonical, &conflict_path]);
    fs::write(&canonical, conflict.local()).unwrap();
    fs::remove_file(&conflict_path).unwrap();
    drop(local);

    let recovered = Archive::open(&root).unwrap();
    assert_eq!(recovered.work(work).unwrap().title(), "External");
    assert_eq!(recovered.conflicts().unwrap().len(), 1);
}

fn write_file_transaction_manifest(root: &std::path::Path, paths: &[&std::path::Path]) {
    let head = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let owned = paths
        .iter()
        .map(|path| {
            let state = if path.exists() {
                serde_json::json!({ "kind": "file", "bytes": fs::read(path).unwrap() })
            } else {
                serde_json::json!({ "kind": "missing" })
            };
            serde_json::json!({
                "path": path.strip_prefix(root).unwrap(),
                "state": state,
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        root.join(".git/carta-transaction.json"),
        serde_json::to_vec(&serde_json::json!({
            "checkpoint": String::from_utf8(head.stdout).unwrap().trim(),
            "owned": owned,
        }))
        .unwrap(),
    )
    .unwrap();
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
