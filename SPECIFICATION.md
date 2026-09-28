# Carta Space Format Specification
## Draft 0.1

**Status:** Experimental / non-stable  
**Working name:** Carta Space  
**Date:** 2026-09-22

This document defines Draft 0.1 of the Carta Space archive format.

Draft 0.x versions are experimental. Backward compatibility is **not** guaranteed until a future 1.0 specification. Implementers are nevertheless encouraged to follow the compatibility rules in this document so that migration experience can inform 1.0.

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**, and **MAY** are used normatively.

---

## 1. Scope

Carta Space is a local document archive format.

This specification defines:

- archive identity;
- directory layout;
- Document storage;
- chronological Volumes;
- Works;
- metadata representation;
- Markdown representation;
- Git history;
- Trash/Wipe requirements;
- portable `.cat` packaging;
- forward-extension behavior.

This specification does **not** define:

- a graphical user interface;
- exact keyboard bindings;
- synchronization;
- collaborative editing;
- tags;
- bibliography;
- images or other asset semantics;
- AI features;
- publication templates.

---

## 2. External standards

Draft 0.1 depends on the following standards:

- **UTF-8** for text encoding.
- **CommonMark 0.31.2** for Document bodies.
- **JSON**, RFC 8259, for structured metadata.
- **UUID version 7**, RFC 9562, for persistent identities.
- **RFC 3339** timestamps for explicit date-time values.
- **Git** for canonical archive history.
- **ZIP** for portable `.cat` packaging.

In Draft 0.1, every textual UUID representation MUST use the canonical lowercase hyphenated form:

```text
xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
```

References are listed in Section 19.

---

## 3. Terminology

### 3.1 Archive

The complete Carta Space information store.

An Archive is both:

1. a directory tree conforming to this specification; and
2. a Git working tree with repository metadata in `.git/`.

### 3.2 Document

A persistent authored textual object.

A Document has:

- a stable UUIDv7 identifier;
- an immutable creation timestamp;
- an effective last-modification timestamp, represented by optional `modified` metadata with `created` as its fallback;
- a CommonMark body.

### 3.3 Volume

A chronological partition identified by calendar year and month.

A Document belongs to the Volume corresponding to its creation timestamp and MUST NOT be moved to a later Volume merely because it is edited later.

### 3.4 Work

An ordered logical composition of Documents.

A Work stores references to Document identifiers. It does not contain copies of Document bodies.

### 3.5 View

A frontend- or core-generated projection derived from Archive state.

A View may select, order, combine, or otherwise present Documents, Works, history, relationships, or search results. A View does not own a canonical copy of authored Document content merely because that content is displayed through the View.

Views are not canonical stored content unless explicitly defined elsewhere.

Draft 0.1 defines no persistent View resource and no `views/` directory.

### 3.6 Current state

The ordinary files present in the Archive working tree.

### 3.7 History

The Git history associated with the Archive.

---

## 4. Archive directory layout

A conforming Draft 0.1 Archive MUST contain the following root entries:

```text
<archive-root>/
├── mimetype
├── carta.json
├── volumes/
├── works/
└── .git/
```

A minimal populated Archive may resemble:

```text
<archive-root>/
├── mimetype
├── carta.json
├── volumes/
│   └── 2026/
│       ├── 09/
│       │   ├── 0199...a1/
│       │   │   ├── content.md
│       │   │   └── meta.json
│       │   └── 0199...b7/
│       │       ├── content.md
│       │       └── meta.json
│       └── 10/
│           └── 019a...21/
│               ├── content.md
│               └── meta.json
├── works/
│   └── 019a...88/
│       └── work.json
└── .git/
```

Implementations MUST NOT require filenames chosen by users for Documents or Works.

Git does not represent empty directories. A writable implementation opening a Git checkout MAY recreate a missing empty `volumes/` or `works/` root only when the current `HEAD` contains no tracked entry beneath that root. A missing root that has tracked descendants in `HEAD` MUST be treated as invalid current state rather than silently reconstructed.

---

## 5. The `mimetype` file

The Archive root MUST contain a UTF-8/ASCII file named exactly:

```text
mimetype
```

Its complete content MUST be:

```text
application/vnd.carta-space+zip
```

with no leading or trailing whitespace.

This media type is provisional in Draft 0.1 and is not asserted to be registered with IANA.

When packaging as `.cat`, `mimetype` SHOULD be the first ZIP member and SHOULD be stored without compression, following a convention proven useful by other ZIP-based document formats.

---

## 6. `carta.json`

The root `carta.json` identifies the Archive format.

Draft 0.1 requires the following members:

```json
{
  "format": "carta-space",
  "format_version": {
    "major": 0,
    "minor": 1
  },
  "archive_id": "019...",
  "created": "2026-09-22T19:00:00+02:00",
  "markdown": "commonmark-0.31.2",
  "history": "git"
}
```

### 6.1 `format`

MUST be the string:

```text
carta-space
```

### 6.2 `format_version`

MUST be an object containing integer `major` and `minor` members.

### 6.3 `archive_id`

MUST be a UUIDv7 string.

### 6.4 `created`

MUST be an RFC 3339 timestamp representing Archive creation time.

### 6.5 `markdown`

MUST be:

```text
commonmark-0.31.2
```

for Draft 0.1.

### 6.6 `history`

MUST be:

```text
git
```

for Draft 0.1.

Additional members MAY appear and MUST be preserved by implementations that rewrite `carta.json`, unless a future specification explicitly defines migration behavior.

---

## 7. Volumes

Volumes are represented by directory paths:

```text
volumes/YYYY/MM/
```

where:

- `YYYY` is a four-digit Gregorian calendar year;
- `MM` is a two-digit month from `01` through `12`.

A Document MUST reside in the Volume derived from the local calendar date represented by its `created` metadata at creation time.

Changing a Document later MUST NOT change its Volume.

Volumes are not user-defined categories.

A conforming implementation MUST NOT require the user to create, name, close, fill, or manually rotate Volumes.

---

## 8. Documents

Each Document is stored in a directory named with its UUIDv7:

```text
volumes/YYYY/MM/<document-uuid>/
```

The directory MUST contain:

```text
content.md
meta.json
```

No other entry is required in Draft 0.1.

Future specifications MAY define additional Document-local resources.

### 8.1 Identity

The directory name MUST equal the `id` value in `meta.json`.

Document identity MUST NOT depend on title, position, pathname beyond the UUID directory, or body contents.

### 8.2 Document body

`content.md` MUST:

- be encoded as UTF-8;
- conform to CommonMark 0.31.2;
- contain authored content only;
- contain no Carta Space administrative front matter required by Draft 0.1.

A Document title, when present, SHOULD be represented in the Markdown body, normally as a heading. Draft 0.1 defines no separate Document title metadata field.

Canonical Document text MUST use LF (`U+000A`) line endings. Writers importing text with CRLF SHOULD normalize it to LF.

Carta Space Draft 0.1 defines no Markdown extensions.

### 8.3 Document metadata

`meta.json` MUST contain:

```json
{
  "id": "019...",
  "created": "2026-09-22T18:24:31+02:00"
}
```

`id` MUST be the Document UUIDv7.

`created` MUST be an RFC 3339 timestamp and MUST be treated as immutable after creation.

Unknown members MUST be preserved when a frontend rewrites the object.

### 8.4 `modified` (optional)

A Document MAY contain a `modified` RFC 3339 timestamp. When absent, the effective modification timestamp is `created`.

A Writer that successfully changes authored Document body content MUST update `modified` to the time of that save. Rewriting identical body content, changing lock state, or changing Work membership MUST NOT by itself update `modified`.

This field is optional for backward compatibility with Draft 0.1 Archives created before modification tracking was introduced.

### 8.5 `locked` (optional)

A Document MAY contain a `locked` Boolean member. `true` means that authored content and structural operations that would alter the Document are locked until an explicit unlock operation. An absent member or `false` means that the Document has no own lock.

A Document MAY still be effectively read-only because it is referenced by a locked Work; that derived state is not duplicated into the Document metadata.

---

## 9. Document boundaries

Document boundaries are structural, not Markdown characters.

A View that concatenates several Documents MUST generate a visible or otherwise perceivable boundary between them.

In an editable multi-Document View, ordinary text editing MUST NOT be able to delete a Document boundary.

A frontend MUST require an explicit structural operation to reorder Documents in a Work or remove a Document reference from a Work.

Draft 0.1 defines one explicit structural split operation. Splitting a Document at a valid UTF-8 byte boundary MUST preserve the original Document identity for the content before the split, create a new UUIDv7 Document for the content at and after the split, and assign the new Document the smallest representable `created` timestamp greater than the original immutable `created` value.

If the source Document belongs to one or more Works, each such Work MUST insert the new Document immediately after the source. This preserves the authored composition order across all projections of the split Document.

Draft 0.1 does not define a Document merge operation.

This requirement prevents accidental corruption of the logical structure while keeping boundary changes explicit.

---

## 10. Works

Each Work is stored in:

```text
works/<work-uuid>/work.json
```

The directory name MUST equal the Work `id`.

A Draft 0.1 `work.json` MUST contain:

```json
{
  "id": "019...",
  "created": "2026-09-22T18:30:00+02:00",
  "title": "War and Peace",
  "documents": [
    "019...A",
    "019...B",
    "019...C"
  ]
}
```

### 10.1 `id`

MUST be a UUIDv7.

### 10.2 `created`

MUST be an RFC 3339 timestamp and SHOULD remain immutable.

### 10.3 `title`

MUST be a JSON string.

A Work title is structural metadata and does not imply that the title is duplicated inside a component Document.

Among active Works, titles MUST be unique according to the following comparison key:

1. remove leading and trailing Unicode whitespace;
2. normalize the result to Unicode NFC;
3. apply Unicode full case folding.

The resulting value is used only for uniqueness comparison. The original authored title MUST be stored unchanged. Implementations MUST NOT use NFKC for this comparison. Trashed historical Works do not reserve their former titles.

### 10.4 `documents`

MUST be a JSON array of Document UUID strings.

Array order is the normative Work order.

Each referenced UUID MUST resolve to an existing current Document.

The same Document UUID MUST NOT occur more than once in a single Work.

The array MAY be empty.

A Work MUST NOT contain copied Document bodies.

A Document MAY be referenced by more than one Work.

### 10.5 `color` (optional)

A Work MAY contain a `color` member as a stable presentation hint.

When written by the reference implementation, it is a string in `#RRGGBB` form. Frontends MAY ignore this hint, but a frontend that rewrites `work.json` MUST preserve it unless the user explicitly changes the Work color.

The color is not part of Work identity, title uniqueness, ordering, or membership semantics.

### 10.6 `locked` (optional)

A Work MAY contain a `locked` Boolean member. `true` locks mutations of the Work structure and makes every currently referenced Document effectively read-only while the Work remains locked. An absent member or `false` means that the Work has no lock.

Unlocking a Work MUST NOT remove any Document's own `locked` state.

Unknown members MUST be preserved when a frontend rewrites `work.json`.

---

## 11. Work View

A frontend MAY present a Work as a continuous editable View.

The View MUST resolve Document references in `documents` order.

Generated separators between component Documents MUST NOT be editable as ordinary body text.

Editing text in a component region MUST edit the referenced Document itself, not a duplicate.

A frontend MAY provide explicit structural commands for insertion, removal, reordering, and the normative Document split defined in section 9.

Draft 0.1 does not define merge interaction semantics.

The exact appearance of boundaries is not specified by the archive format.

---

## 12. Creation Date and Modification Date Views

A frontend SHOULD be capable of presenting the Documents of a Volume in Creation Date View.

Creation Date View orders Documents from oldest to newest by the explicit `created` timestamp. UUIDv7 ordering MAY be used as an implementation aid but MUST NOT replace `created` as the semantic creation timestamp.

A frontend MAY provide larger creation-time ranges spanning several Volumes.

A frontend MAY provide Modification Date View across active Documents. When provided, it orders Documents from least recently modified to most recently modified by the effective modification timestamp defined in section 8.4.

A frontend MAY provide other derived Views, including search-result Views, relationship or backlink Views, and history-oriented Views.

A derived View MUST NOT require duplication of canonical Document bodies. Loss of a View-specific cache or index MUST NOT cause loss of authored or structural Archive data.

Draft 0.1 does not define persistence or interchange semantics for user-saved Views.

---

## 13. Markdown semantics

Draft 0.1 uses CommonMark 0.31.2 without extensions.

Carta Space treats Markdown primarily as authored semantics rather than final page layout.

A frontend SHOULD allow users to express CommonMark structures without requiring them to manage typography.

Draft 0.1 does not define:

- fonts;
- point sizes;
- physical margins;
- page dimensions;
- print pagination;
- stylesheet semantics.

Those belong to rendering or publication layers outside the core archive format.

A future specification MAY add explicitly named Markdown extensions. Such extensions MUST be versioned.

### 13.1 Internal Carta links

Draft 0.1 MAY represent internal references using ordinary CommonMark links. No additional Markdown syntax is introduced.

The following link destinations are reserved:

```text
carta:doc:<DocumentId>
carta:work:<WorkId>
```

`<DocumentId>` and `<WorkId>` use the canonical textual UUID representation of the corresponding Carta identifier.

The visible CommonMark link label is authored content and MUST NOT be used as the target identity. Changing a Document heading or Work title therefore does not change an internal link target.

An implementation MAY resolve these links for navigation when the target exists.

An unresolved internal Carta link MUST NOT by itself make an Archive structurally invalid. This differs from a Document reference in a Work's `documents` array, which MUST resolve according to Section 10.

Backlinks MUST NOT be required as canonical metadata in Draft 0.1. An implementation MAY derive backlink relationships by scanning canonical Document bodies or by maintaining a disposable derived index.

---

## 14. Git history

A writable conforming Carta Space Draft 0.1 Archive MUST be a Git repository.

The `.git/` directory contains canonical Archive history.

### 14.1 User-interface independence

The format requirement does not require the user interface to expose Git concepts.

A frontend MAY use the Git command-line implementation, libgit2, JGit, or another conforming Git implementation.

### 14.2 Current state versus history

The filesystem working tree is the current state.

Git commits are historical checkpoints.

Autosaving the current state and committing history are distinct operations.

### 14.3 Commit policy

Draft 0.1 does not prescribe exact checkpoint frequency.

A writable frontend SHOULD create automatic checkpoints frequently enough to make meaningful historical recovery possible without creating a commit for every keystroke.

A frontend SHOULD create a checkpoint when cleanly closing an active session if the Archive has changes.

### 14.4 Packaging

Before exporting a portable `.cat` package, a writer MUST ensure that the packaged Git history contains a commit representing the packaged current state.

Device-local Git configuration is not canonical Archive content. In particular, synchronization remotes and credentials MUST NOT be required for archive reconstruction and SHOULD NOT be included in a portable package.

### 14.5 Readers

A read-only implementation MAY ignore `.git/` and still read current Documents and Works.

A writable implementation claiming full Draft 0.1 support MUST preserve Git history.

---

## 15. Trash and Wipe

Carta Space distinguishes recoverable removal from permanent history erasure. The v0.1 user-facing terms are **Trash** and **Wipe**.

### 15.1 Trash

Trashing removes a Document from the current working tree while leaving it recoverable from Git history.

Before Trash completes, all current Work references to that Document MUST be removed by an explicit user-visible structural operation. A current Work MUST NOT contain a dangling structural Document reference.

Authored Markdown links to the Document MUST NOT be silently rewritten merely because the target was trashed. Such links may remain unresolved until the target is restored or the authored text is changed.

A frontend MAY maintain a disposable derived Trash index. Draft 0.1 defines no canonical `trash/` hierarchy.

### 15.2 Restore

Restoring a trashed Document restores the same Document identity, immutable `created` timestamp, original Volume assignment, and selected historical content. Former Work memberships MUST NOT be restored silently.

### 15.3 Wipe

A conforming writable implementation MUST provide, either directly or through an administrative tool, a means to Wipe a trashed Document from the Archive's retained history.

A Wipe MUST:

1. require the Document to be absent from the active working tree;
2. remove retained Git references/history containing the Document;
3. remove relevant unreachable Git objects and reflog reachability as required to complete removal under Carta's control;
4. remove Carta-managed derived indexes, caches, autosave state, and temporary/session copies that contain the wiped content.

After successful Wipe on the current device, Carta Space itself MUST NOT be able to recover the Document from that device.

A Wipe applies only to data under Carta Space's control on the device where it is performed. It cannot guarantee removal from:

- external backups;
- previously exported `.cat` packages;
- cloned repositories or other devices;
- copied files;
- filesystem/storage snapshots;
- physical storage remnants outside the application's control.

A frontend MUST clearly distinguish Wipe from Trash because Wipe is destructive.

---

## 16. Extensibility and preservation

Draft 0.1 intentionally defines a small core.

Future versions MAY add:

- JSON members;
- files;
- directories;
- optional derived indexes;
- additional resource types;
- Markdown extensions.

A Draft 0.1 implementation encountering unknown JSON members SHOULD preserve them when rewriting the containing object.

A Draft 0.1 implementation encountering unknown files or directories SHOULD preserve them unless the user explicitly requests destructive cleanup.

Canonical authored data MUST NOT depend solely on an optional derived index or cache.

Optional derived indexes MAY include full-text indexes, internal-link/backlink indexes, vector or semantic-search indexes, and other acceleration structures.

Loss or deletion of a derived index MUST NOT cause loss of authored text, identities, Work structure, or Git history. Implementations MAY rebuild functionally equivalent derived indexes from canonical Archive data. Model-dependent derived data, including embeddings, is not part of Draft 0.1 conformance.

Future extensions SHOULD be additive where practical.

---

## 17. Portable `.cat` package

A `.cat` file is a ZIP archive containing the Carta Space Archive root.

The extension is:

```text
.cat
```

The package MUST include:

- `mimetype`;
- `carta.json`;
- `volumes/`;
- `works/`;
- `.git/`.

The `mimetype` entry SHOULD be the first ZIP entry and SHOULD be uncompressed.

Package creation SHOULD be atomic from the user's perspective: an implementation SHOULD write and validate a temporary package before replacing an existing destination package.

The ZIP container is an interchange representation. Implementations SHOULD edit the unpacked working Archive rather than rewriting the ZIP for each text modification.

---

## 18. Conformance classes

Draft 0.1 defines three useful conformance descriptions.

### 18.1 Reader

A Reader can:

- read `carta.json`;
- enumerate Documents;
- read `meta.json`;
- parse CommonMark bodies;
- resolve Works.

A Reader MAY ignore Git history.

### 18.2 Writer

A Writer satisfies Reader requirements and additionally:

- preserves stable identities;
- creates valid UUIDv7 values;
- preserves required metadata;
- respects Volume assignment;
- prevents dangling Work references;
- preserves unknown supported data according to Section 16;
- maintains Git history.

### 18.3 Full interactive frontend

A full frontend is a Writer that additionally provides an interactive editing environment.

Keyboard bindings, LEAP interaction design, rendering behavior, and publication tools are outside storage-format conformance. The accepted v0.1 reference interaction semantics are defined separately in `INTERACTION-CONTRACT.md`.

---

## 19. References

### CommonMark

CommonMark Specification 0.31.2  
https://spec.commonmark.org/0.31.2/

### JSON

RFC 8259 — The JavaScript Object Notation (JSON) Data Interchange Format  
https://www.rfc-editor.org/rfc/rfc8259

### UUID

RFC 9562 — Universally Unique IDentifiers (UUIDs), including UUIDv7  
https://www.rfc-editor.org/rfc/rfc9562

### Date and time

RFC 3339 — Date and Time on the Internet: Timestamps  
https://www.rfc-editor.org/rfc/rfc3339

### Git

Git documentation  
https://git-scm.com/docs

### EPUB

EPUB 3.3 — Package Document and Spine concepts  
https://www.w3.org/TR/epub-33/

---

## 20. Open questions for Draft 0.2

The following questions are intentionally not resolved by Draft 0.1:

1. Exact automatic Git checkpoint policy.
2. Whether a deleted Document requires an explicit tombstone in addition to Git history.
3. Exact rules for displaying and editing multi-Document Views.
4. LEAP scope-selection interaction.
5. Publication-profile representation.
6. Whether Draft 1.0 should define a reserved namespace for extensions.
7. Whether portable packages should include a Git bundle instead of raw `.git/` metadata in a future revision.

These questions MUST NOT be silently resolved by embedding incompatible assumptions into the core format during the first prototype.
