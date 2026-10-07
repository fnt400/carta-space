use std::str::FromStr;

use carta_core::{
    extract_markdown_links, link_at_byte_offset, Archive, CartaLinkTarget, DocumentId,
    DocumentTextRegion, Error, LeapDirection, LeapPosition, LeapRuntime, LeapSession,
    LinkResolution, WorkId,
};

fn create_archive() -> (tempfile::TempDir, Archive) {
    let temporary = tempfile::tempdir().unwrap();
    let archive = Archive::create(temporary.path().join("archive")).unwrap();
    (temporary, archive)
}

#[test]
fn derives_labels_from_only_the_first_non_empty_line() {
    let (_temporary, mut archive) = create_archive();
    let heading = archive
        .create_document("\n  \n# A **rich** `heading` #\nLater")
        .unwrap();
    let setext = archive
        .create_document("Setext *label*\n=====\nBody")
        .unwrap();
    let plain = archive
        .create_document("\n  ordinary **Markdown** line  \n# Later heading")
        .unwrap();
    let empty = archive.create_document(" \n\t\n").unwrap();

    assert_eq!(
        archive.read_document(heading).unwrap().derived_label(),
        "A rich heading"
    );
    assert_eq!(
        archive.read_document(setext).unwrap().derived_label(),
        "Setext label"
    );
    assert_eq!(
        archive.read_document(plain).unwrap().derived_label(),
        "  ordinary **Markdown** line  "
    );
    assert_eq!(
        archive.read_document(empty).unwrap().derived_label(),
        archive
            .documents()
            .find(|document| document.id() == empty)
            .unwrap()
            .created()
            .to_string()
    );
}

#[test]
fn parses_only_commonmark_links_and_canonical_carta_targets() {
    let document = DocumentId::new_v7();
    let work = WorkId::new_v7();
    let markdown = format!(
        "literal carta:doc:{document}\n[doc *label*](carta:doc:{document} \"title\") and <https://example.com> and [work][w]\n\n[w]: carta:work:{work}\n[bad](carta:doc:{})",
        document.to_string().to_uppercase()
    );
    let links = extract_markdown_links(&markdown);

    assert_eq!(links.len(), 4);
    assert_eq!(
        links[0].carta_target(),
        Some(CartaLinkTarget::Document(document))
    );
    assert_eq!(links[1].destination(), "https://example.com");
    assert_eq!(links[1].carta_target(), None);
    assert_eq!(links[2].carta_target(), Some(CartaLinkTarget::Work(work)));
    assert_eq!(links[3].carta_target(), None);

    let inside = markdown.find("doc *label*").unwrap() + 2;
    assert_eq!(
        link_at_byte_offset(&markdown, inside)
            .unwrap()
            .carta_target(),
        Some(CartaLinkTarget::Document(document))
    );
    assert!(link_at_byte_offset(&markdown, markdown.find("literal").unwrap()).is_none());
    assert!(link_at_byte_offset("é", 1).is_none());
}

#[test]
fn resolves_links_and_derives_backlinks_from_disposable_index() {
    let (_temporary, mut archive) = create_archive();
    let target = archive.create_document("target").unwrap();
    let work = archive.create_empty_work("Target work".to_owned()).unwrap();
    let missing = DocumentId::from_str("01890f3e-70a9-7cc3-98c4-dc0c0c073990").unwrap();
    let source = archive
        .create_document(&format!(
            "[one](carta:doc:{target}) [two](carta:doc:{target}) [work](carta:work:{work}) [gone](carta:doc:{missing})"
        ))
        .unwrap();

    let links = archive.document_links(source).unwrap();
    assert_eq!(
        archive.resolve_link(&links[0]),
        Some(LinkResolution::Document(target))
    );
    assert_eq!(
        archive.resolve_link(&links[2]),
        Some(LinkResolution::Work(work))
    );
    assert_eq!(
        archive.resolve_link(&links[3]),
        Some(LinkResolution::Unresolved(CartaLinkTarget::Document(
            missing
        )))
    );
    let backlinks = archive
        .backlinks(CartaLinkTarget::Document(target))
        .unwrap();
    assert_eq!(backlinks.len(), 2);
    assert!(backlinks.iter().all(|backlink| backlink.source() == source));
    assert!(backlinks
        .iter()
        .all(|backlink| backlink.label().contains("one")));
    assert!(backlinks
        .iter()
        .all(|backlink| backlink.context().contains("carta:doc:")));

    archive
        .edit_document(source, &format!("[only](carta:doc:{target})"))
        .unwrap();
    let backlinks = archive
        .backlinks(CartaLinkTarget::Document(target))
        .unwrap();
    assert_eq!(backlinks.len(), 1);
    assert_eq!(
        backlinks[0].context(),
        format!("[only](carta:doc:{target})")
    );
}

#[test]
fn persistent_backlink_cache_is_reused_rebuilt_and_disposable() {
    let (temporary, mut archive) = create_archive();
    let root = archive.root().to_path_buf();
    let cache_root = temporary.path().join("cache");
    let target = archive.create_document("# Target").unwrap();
    let source = archive
        .create_document(&format!("# Source\n\n[first](carta:doc:{target})"))
        .unwrap();
    drop(archive);

    let archive = Archive::open_with_backlink_cache(&root, &cache_root).unwrap();
    assert_eq!(
        archive
            .backlinks(CartaLinkTarget::Document(target))
            .unwrap()
            .len(),
        1
    );
    let cache_path = cache_root
        .join("backlinks-v1")
        .join(format!("{}.json", archive.metadata().archive_id()));
    assert!(cache_path.is_file());
    let source_path = archive
        .documents()
        .find(|info| info.id() == source)
        .unwrap()
        .path()
        .join("content.md");
    drop(archive);

    std::fs::write(
        &source_path,
        format!("# Source changed\n\n[first](carta:doc:{target}) [second](carta:doc:{target})\n"),
    )
    .unwrap();
    let archive = Archive::open_with_backlink_cache(&root, &cache_root).unwrap();
    assert_eq!(
        archive
            .backlinks(CartaLinkTarget::Document(target))
            .unwrap()
            .len(),
        2
    );
    drop(archive);

    std::fs::write(&cache_path, b"{ definitely not valid json").unwrap();
    let archive = Archive::open_with_backlink_cache(&root, &cache_root).unwrap();
    assert_eq!(
        archive
            .backlinks(CartaLinkTarget::Document(target))
            .unwrap()
            .len(),
        2
    );
    assert!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(&cache_path).unwrap()).is_ok()
    );

    std::fs::remove_file(&cache_path).unwrap();
    let archive = Archive::open_with_backlink_cache(&root, &cache_root).unwrap();
    assert_eq!(
        archive
            .backlinks(CartaLinkTarget::Document(target))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn archive_search_is_literal_grouped_contextual_and_newest_first() {
    let (_temporary, mut archive) = create_archive();
    let first = archive
        .create_document("# First\n\nBefore Straße after\nsecond STRASSE")
        .unwrap();
    let second = archive.create_document("Newest strasse context").unwrap();
    archive.create_document("strasse\nnext").unwrap();

    let results = archive.search("  STRASSE  ").unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(
        results[0].document(),
        archive.documents().last().unwrap().id()
    );
    assert_eq!(results[1].document(), second);
    assert_eq!(results[2].document(), first);
    assert_eq!(results[2].label(), "First");
    assert_eq!(
        &archive.read_document(first).unwrap().content()[results[2].occurrence()],
        "Straße"
    );
    assert_eq!(results[2].context(), "Before Straße after");
    assert_eq!(
        &archive.read_document(first).unwrap().content()[results[2].context_range()],
        results[2].context()
    );
    assert!(matches!(
        archive.search("Straße after\nsecond"),
        Err(Error::MultilineSearchQuery)
    ));
    assert!(archive.search("Straße  after").unwrap().is_empty());
    assert!(archive.search("  \t ").unwrap().is_empty());
}

#[test]
fn leap_is_incremental_directional_circular_and_document_bounded() {
    let first = DocumentId::new_v7();
    let second = DocumentId::new_v7();
    let regions = [
        DocumentTextRegion::new(first, "alpha end a"),
        DocumentTextRegion::new(second, "b ALPHA omega"),
    ];
    let origin = LeapPosition::new(0, 6);
    let mut leap = LeapSession::new(LeapDirection::Forward, origin);

    leap.push_str("al", &regions);
    assert_eq!(leap.cursor(), LeapPosition::new(1, 2));
    assert_eq!(leap.current_match().unwrap().document(), second);
    assert!(!leap.current_match().unwrap().wrapped());
    leap.push_str("z", &regions);
    assert_eq!(leap.cursor(), origin);
    assert!(leap.current_match().is_none());
    assert!(leap.backspace(&regions));
    assert_eq!(leap.cursor(), LeapPosition::new(1, 2));
    assert!(leap.backspace(&regions));
    assert_eq!(leap.query(), "a");

    let wrapped = LeapSession::with_query(
        LeapDirection::Forward,
        LeapPosition::new(1, regions[1].text().len()),
        "alpha",
        &regions,
    );
    assert_eq!(wrapped.cursor(), LeapPosition::new(0, 0));
    assert!(wrapped.current_match().unwrap().wrapped());

    let backward = LeapSession::with_query(
        LeapDirection::Backward,
        LeapPosition::new(1, regions[1].text().len()),
        "ALPHA",
        &regions,
    );
    assert_eq!(backward.cursor(), LeapPosition::new(1, 2));
    assert!(!backward.current_match().unwrap().wrapped());

    let crossing = LeapSession::with_query(
        LeapDirection::Forward,
        LeapPosition::new(0, 0),
        "ab",
        &regions,
    );
    assert!(crossing.current_match().is_none());
    assert_eq!(crossing.cursor(), LeapPosition::new(0, 0));
}

#[test]
fn leap_runtime_remembers_only_query_and_again_skips_then_wraps() {
    let document = DocumentId::new_v7();
    let regions = [DocumentTextRegion::new(document, "one ONE")];
    let session = LeapSession::with_query(
        LeapDirection::Forward,
        LeapPosition::new(0, 0),
        "one",
        &regions,
    );
    let mut runtime = LeapRuntime::default();
    runtime.remember(&session);

    let next = runtime
        .leap_again(LeapDirection::Forward, session.cursor(), &regions)
        .unwrap();
    assert_eq!(next.position(), LeapPosition::new(0, 4));
    assert!(!next.wrapped());
    let wrapped = runtime
        .leap_again(LeapDirection::Forward, next.position(), &regions)
        .unwrap();
    assert_eq!(wrapped.position(), LeapPosition::new(0, 0));
    assert!(wrapped.wrapped());
    assert_eq!(runtime.remembered_query(), Some("one"));
}

#[test]
fn imports_utf8_bytes_neutrally_and_exports_exact_markdown() {
    let (_temporary, mut archive) = create_archive();
    let imported = archive
        .import_document_bytes(b"\xef\xbb\xbf# Kept\r\n[pasted](carta:doc:literal)\rfinal")
        .unwrap();
    assert_eq!(
        archive.export_document_markdown(imported).unwrap(),
        "# Kept\n[pasted](carta:doc:literal)\nfinal\n"
    );
    assert!(archive.memberships(imported).unwrap().is_empty());
    assert!(matches!(
        archive.import_document_bytes(&[0xff]),
        Err(Error::InvalidImportUtf8(_))
    ));

    let second = archive.create_document("\nSecond body").unwrap();
    let work = archive
        .create_work("Export".to_owned(), vec![second, imported])
        .unwrap();
    assert_eq!(
        archive.export_work_markdown(work).unwrap(),
        "\nSecond body\n# Kept\n[pasted](carta:doc:literal)\nfinal\n"
    );
}
