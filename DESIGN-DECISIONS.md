# Carta Space — Design Decisions
## Draft 0.1

This document records architectural decisions and the reasoning behind them.

It is intentionally non-normative. `SPECIFICATION.md` is authoritative for the format.

---

## DD-001 — Keep the core deliberately small

**Decision:** Carta Space 0.x will implement only the concepts required to test the central model: Documents, monthly Volumes, Works, CommonMark semantics, Git history, and content-oriented navigation.

**Why:** The project is explicitly about reducing conceptual overhead. Adding tags, assets, bibliography, synchronization, semantic search, or AI before demonstrated need would undermine that goal and multiply implementation failure modes.

**Rule of thumb:** Complexity must be earned by a demonstrated problem.

---

## DD-002 — Use standard technologies rather than inventing a low-level format

**Decision:** Use UTF-8, CommonMark, JSON, UUIDv7, Git, and ZIP.

**Why:** Carta Space's contribution should be the information model and interaction model, not another parser, identifier scheme, archive format, or revision-control engine.

This also maximizes recoverability with generic tools.

---

## DD-003 — CommonMark 0.31.2 is the canonical Document body syntax

**Decision:** `content.md` uses CommonMark 0.31.2 without extensions in Draft 0.1.

**Alternatives considered:**

- original informal Markdown;
- GitHub Flavored Markdown;
- Pandoc Markdown;
- Org mode;
- custom rich-text metadata.

**Why CommonMark:** It has a precise specification and test suite while staying close to simple readable Markdown.

**Why not GFM:** Its additional syntax solves GitHub-specific and collaboration-oriented problems that Carta Space does not yet have.

**Why not Pandoc Markdown:** It is powerful precisely because it has many extensions. Adopting the whole dialect would prematurely enlarge the language.

**Why not Org:** The storage format must remain independent of any editor or frontend. Emacs may later be supported as an alternate frontend, but it is not the reference implementation.

**Why not separate rich-text ranges:** Range metadata becomes fragile when text is edited outside the reference frontend.

Pandoc remains a strong candidate for conversion and publishing.

---

## DD-004 — Administrative metadata stays outside Markdown

**Decision:** Document administration uses `meta.json`; no required YAML or JSON front matter is placed in `content.md`.

**Why:** An extracted `content.md` should contain authored content, not archive bookkeeping.

Document title is content and can therefore be expressed with a Markdown heading. UUID and creation timestamp are administrative facts and belong in metadata.

---

## DD-005 — Use JSON sidecars

**Decision:** Metadata uses RFC 8259 JSON.

**Alternatives considered:** YAML, TOML, XML, custom key/value syntax.

**Why:** JSON is standardized, language-independent, ubiquitous, small enough for the required structures, and suitable for data written primarily by software rather than by humans.

The core metadata model is intentionally too small to justify a more elaborate format.

---

## DD-006 — Use UUIDv7 for identities

**Decision:** Archives, Documents, and Works use UUIDv7.

**Why:** Stable opaque identities allow content to move through different logical views without identity depending on filenames, titles, or text.

UUIDv7 is standardized by RFC 9562 and includes temporal ordering properties useful for implementation while remaining globally unique.

Creation time is still stored explicitly. The UUID is identity, not the sole semantic timestamp.

---

## DD-007 — Monthly Volumes are automatic chronological partitions

**Decision:** Physical Document directories are grouped under `volumes/YYYY/MM/` according to creation month.

**Alternatives considered:**

- manually created Volumes;
- size-limited Volumes inspired directly by floppy capacity;
- topic folders;
- one unlimited directory.

**Why monthly:** Calendar months provide predictable bounded contexts without requiring the user to classify or maintain them.

A Volume answers "when did this enter my archive?", not "what is this about?"

Documents remain in their creation Volume after later edits.

---

## DD-008 — Works are ordered references, not containers of copied text

**Decision:** A Work stores an ordered array of Document UUIDs.

**Why:** This separates intellectual order from chronological storage.

The design is analogous in spirit to EPUB's separation between publication resources and its ordered spine.

A Document can therefore appear in more than one Work without duplication.

---

## DD-009 — Document boundaries in composite Views are protected

**Decision:** Boundaries shown between Documents in a Work View or chronological View are generated structural elements, not characters stored in Markdown.

**Why:** If a separator could be erased by Backspace, ordinary text editing could accidentally destroy archive structure.

Merging, splitting, and reordering are explicit operations.

---

## DD-010 — Git is the canonical history mechanism

**Decision:** A writable Draft 0.1 Archive is a Git repository.

**Earlier alternative:** Leave the choice of version-control engine to each frontend.

**Why it was rejected:** Frontends using incompatible history systems would interoperate only on current content, defeating the goal of a common archival format.

Git is widely implemented, mature, efficient for text, and independently inspectable.

The normal Carta Space interface must nevertheless hide Git concepts from users.

---

## DD-011 — Autosave and history are different

**Decision:** Current files are saved frequently; Git checkpoints occur separately.

**Why:** A commit for every keystroke would be wasteful and conceptually wrong. Autosave protects recent work from failure. History records meaningful recoverable states.

Exact checkpoint policy is intentionally deferred until prototype use provides evidence.

---

## DD-012 — Ordinary deletion and permanent purge are separate

**Decision:** Normal deletion may remain recoverable from Git. Permanent purge rewrites history.

**Why:** Version history is valuable only if accidental deletion remains recoverable, but users also need a way to remove sensitive or unwanted content from the archive itself.

Purge cannot promise deletion from independent external copies or backups.

---

## DD-013 — Git history belongs to the portable archive

**Decision:** Draft 0.1 `.cat` packages include archive history.

**Why:** Two copies of the same Carta Space archive should not silently have different notions of its past merely because one was exported.

**Open issue:** A future specification may prefer a Git bundle over raw `.git/` metadata for cleaner interchange. The initial prototype should test the simplest representation first.

---

## DD-014 — Working directory and portable package are different forms

**Decision:** Editing happens in an ordinary directory tree. `.cat` is a ZIP-based transport package.

**Why:** Continually rewriting a compressed archive would complicate autosave, failure recovery, Git, and external inspection.

The package is created atomically when required.

---

## DD-015 — Semantic authoring precedes presentation

**Decision:** The core records semantic text structure, not page typography.

**Why:** Authors should state that something is a heading, quotation, emphasis, or list without deciding its exact font and geometry during composition.

Printing and publication are rendering operations.

LaTeX is a likely rendering backend for high-quality print/PDF output, but it is not part of the canonical storage model.

---

## DD-016 — No tags in the initial core

**Decision:** Draft 0.1 has no tag or keyword system.

**Why:** LEAP, full-text content, chronological scope, Git history, and Works should first be tested as retrieval mechanisms.

If real use demonstrates a recurring retrieval problem that tags solve, they can be designed from evidence.

---

## DD-017 — No synchronization or concurrent editing initially

**Decision:** The first implementation is local, single-user, single-writer.

**Why:** Synchronization would introduce merge conflicts, distributed identity, locking, causality, and network failure before the central interaction model has been validated.

The underlying format should avoid choices that make future synchronization impossible, but it does not solve that problem now.

---

## DD-018 — Unknown future data must survive old frontends

**Decision:** Frontends should preserve unknown metadata members and unknown archive resources whenever possible.

**Why:** Extensibility is useful only if adding a new feature does not make older tools destructive.

Future capabilities should be additive where practical.

No empty `assets`, `tags`, `bibliography`, `ai`, or similar directories are created pre-emptively.

---

## DD-019 — Canonical authored data must not depend on an index

**Decision:** Search indexes, databases, caches, embeddings, or other acceleration structures are always derived data unless a future specification explicitly changes this rule.

**Why:** The archive must remain reconstructable from human-readable canonical files.

---

## DD-020 — LEAP is an interface concept, not a storage-format property

**Decision:** Carta Space defines two momentary LEAP controls, backward and forward. The initial terminal frontend will experimentally map them to the physical left and right Control keys.

**Why:** AltGr is required for normal text entry on common international keyboard layouts, making the two Alt keys a poor default. The two Control keys are physically convenient and preserve the intended two-handed quasimodal interaction. Modern terminal keyboard protocols can distinguish the physical controls and report press/release events.

Exact key bindings remain outside the storage-format specification. A frontend on a platform that cannot expose the two physical Control keys may choose a different mapping.

---

## Outstanding design risks

The first prototype must test rather than theorize away three major risks:

1. whether LEAP plus temporal scope and Works remains effective over a large archive;
2. whether users naturally choose useful Document boundaries without excessive management;
3. whether a continuous editable Work View can remain simple while safely projecting edits into multiple underlying Markdown Documents.

No additional organizational system should be introduced until one of these risks produces a concrete problem.

---

## DD-021 — Rust is the reference implementation language

**Decision:** The first reference implementation will be written in Rust.

**Why:** Carta Space is intended to be a durable standalone system rather than an editor-specific package. Rust provides strong types for the format model, explicit error handling, good testing support, efficient Unicode/text processing, mature serialization libraries, straightforward CLI distribution, and the ability to share one core across multiple frontends.

This is an implementation decision, not a format requirement. Independent implementations may use any language.

---

## DD-022 — Separate format, core, CLI, and frontend

**Decision:** The reference implementation will be split into at least four logical components: `carta-format`, `carta-core`, `carta-cli`, and `carta-tui`.

**Why:** Archive semantics must not become entangled with one user interface.

The core should be testable without a terminal or GUI. The CLI should make administrative and scripted operations available in a conventional Unix form. Interactive frontends should use the same core operations rather than maintaining a second implementation of Documents, Works, history, or purge.

This separation is also the foundation for possible future GTK, Emacs, web, or other frontends.

---

## DD-023 — The first interactive frontend is a TUI

**Decision:** The initial interactive frontend will be a full-screen terminal application built with Ratatui and Crossterm rather than GTK.

**Why:** Carta Space is primarily a keyboard-driven text environment and deliberately separates writing from final page presentation. A terminal UI therefore matches the project's minimal interaction model, works naturally over SSH, has low runtime overhead, and discourages premature addition of toolbars, panels, and presentation-oriented controls.

**Trade-off:** Traditional terminal protocols do not reliably expose physical left/right modifiers or key-release events. The reference TUI will therefore rely on modern enhanced keyboard reporting when available and must detect when the required event fidelity is unavailable.

A graphical frontend remains a possible future frontend, not a competing core implementation.

---

## DD-024 — Left and right Control are the initial LEAP bindings

**Decision:** In the reference TUI, physical left Control is the default experimental binding for LEAP backward and physical right Control for LEAP forward.

**Alternatives considered:** left/right Alt, function keys, Caps Lock, dedicated remapped keys.

**Why not Alt:** On international keyboard layouts AltGr is required for ordinary character entry, including accented characters.

**Why not function keys or Caps Lock:** They are less attractive ergonomically for the intended momentary, frequently held LEAP gesture.

**Why Control:** The two keys are physically distinct, convenient to hold, and available on standard keyboards.

This remains an experiment. Real use may justify a different default. Carta Space itself defines two LEAP controls, not two specific keyboard scancodes.
