use std::str::FromStr;

use carta_format::{
    ArchiveMetadata, DocumentId, DocumentMetadata, FormatError, Timestamp, WorkMetadata,
};

const VALID_ARCHIVE: &str = include_str!("fixtures/carta-valid.json");
const UNKNOWN_FIELDS: &str = include_str!("fixtures/carta-unknown-fields.json");
const VALID_DOCUMENT: &str = include_str!("fixtures/document-valid.json");
const VALID_WORK: &str = include_str!("fixtures/work-valid.json");

#[test]
fn reads_draft_0_1_metadata() {
    let archive = ArchiveMetadata::read_from(VALID_ARCHIVE.as_bytes()).unwrap();
    let document = DocumentMetadata::read_from(VALID_DOCUMENT.as_bytes()).unwrap();
    let work = WorkMetadata::read_from(VALID_WORK.as_bytes()).unwrap();

    assert_eq!(archive.format_version().major(), 0);
    assert_eq!(archive.format_version().minor(), 1);
    assert_eq!(document.created().year(), 2026);
    assert_eq!(work.title(), "War and Peace");
    assert_eq!(work.documents(), &[document.id()]);
}

#[test]
fn preserves_unknown_json_members_and_large_numbers() {
    let metadata = ArchiveMetadata::read_from(UNKNOWN_FIELDS.as_bytes()).unwrap();
    let mut output = Vec::new();
    metadata.write_to(&mut output).unwrap();
    let rewritten: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let original: serde_json::Value = serde_json::from_str(UNKNOWN_FIELDS).unwrap();

    assert_eq!(rewritten["future_integer"], original["future_integer"]);
    assert_eq!(rewritten["future_object"], original["future_object"]);
    assert_eq!(
        rewritten["format_version"]["future_version_member"],
        original["format_version"]["future_version_member"]
    );
}

#[test]
fn preserves_unknown_document_and_work_members() {
    let document_input = VALID_DOCUMENT.replace(
        "\n}",
        ",\n  \"future_document_member\": { \"value\": 7 }\n}",
    );
    let work_input = VALID_WORK.replace("\n}", ",\n  \"future_work_member\": [true, false]\n}");

    let document = DocumentMetadata::read_from(document_input.as_bytes()).unwrap();
    let work = WorkMetadata::read_from(work_input.as_bytes()).unwrap();
    let mut document_output = Vec::new();
    let mut work_output = Vec::new();
    document.write_to(&mut document_output).unwrap();
    work.write_to(&mut work_output).unwrap();

    let document_json: serde_json::Value = serde_json::from_slice(&document_output).unwrap();
    let work_json: serde_json::Value = serde_json::from_slice(&work_output).unwrap();
    assert_eq!(
        document_json["future_document_member"],
        serde_json::json!({"value": 7})
    );
    assert_eq!(
        work_json["future_work_member"],
        serde_json::json!([true, false])
    );
}

#[test]
fn rejects_noncanonical_or_non_v7_ids() {
    let canonical = "01890f3e-70a9-7cc3-98c4-dc0c0c073990";
    assert!(DocumentId::from_str(canonical).is_ok());
    assert!(DocumentId::from_str(&canonical.to_uppercase()).is_err());
    assert!(DocumentId::from_str("550e8400-e29b-41d4-a716-446655440000").is_err());
    assert!(DocumentId::from_str("not-a-uuid").is_err());
}

#[test]
fn timestamps_use_the_calendar_date_in_the_stored_offset() {
    let timestamp = Timestamp::from_str("2026-10-01T00:30:00+02:00").unwrap();
    assert_eq!(timestamp.year(), 2026);
    assert_eq!(timestamp.month(), 10);
    assert_eq!(timestamp.to_string(), "2026-10-01T00:30:00+02:00");
}

#[test]
fn rejects_unsupported_versions_and_fixed_values() {
    let wrong_version = VALID_ARCHIVE.replace("\"minor\": 1", "\"minor\": 2");
    assert!(matches!(
        ArchiveMetadata::read_from(wrong_version.as_bytes()),
        Err(FormatError::UnsupportedVersion { major: 0, minor: 2 })
    ));

    let wrong_format = VALID_ARCHIVE.replace("carta-space", "other-format");
    assert!(matches!(
        ArchiveMetadata::read_from(wrong_format.as_bytes()),
        Err(FormatError::InvalidFormatName { .. })
    ));
}

#[test]
fn rejects_trailing_json_data() {
    let input = format!("{VALID_DOCUMENT} true");
    assert!(DocumentMetadata::read_from(input.as_bytes()).is_err());
}
