use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn create_validate_and_inspect_have_scriptable_success_output() {
    let temporary = tempfile::tempdir().unwrap();
    let archive = temporary.path().join("archive");

    Command::cargo_bin("carta")
        .unwrap()
        .args(["create", archive.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains(archive.to_str().unwrap()));
    Command::cargo_bin("carta")
        .unwrap()
        .args(["--archive", archive.to_str().unwrap(), "validate"])
        .assert()
        .success()
        .stdout("valid\n");
    Command::cargo_bin("carta")
        .unwrap()
        .args(["--archive", archive.to_str().unwrap(), "inspect"])
        .assert()
        .success()
        .stdout(predicate::str::contains("documents\t0"));
}

#[test]
fn invalid_archive_and_usage_errors_are_nonzero() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(temporary.path().join("mimetype"), b"wrong").unwrap();

    Command::cargo_bin("carta")
        .unwrap()
        .args(["--archive", temporary.path().to_str().unwrap(), "validate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required entry is missing"));
    Command::cargo_bin("carta")
        .unwrap()
        .args(["trash", "document"])
        .assert()
        .code(2);
}

#[test]
fn missing_required_trash_confirmation_is_a_usage_error() {
    let id = carta_core::DocumentId::new_v7();
    Command::cargo_bin("carta")
        .unwrap()
        .args(["trash", "document", &id.to_string()])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("--confirm"));
}
