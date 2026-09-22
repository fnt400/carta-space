# Carta Space
## A content-first document environment inspired by Jef Raskin

**Status:** Draft 0.1  
**Working name:** Carta Space  
**Date:** 2026-09-22

## Abstract

Carta Space is an experimental document environment based on a simple premise: personal computing has traditionally made the user manage too much of the computer's internal organization.

Files, directories, applications, save operations, document locations, and presentation settings are often implementation concerns exposed as user responsibilities. Jef Raskin's work on the Canon Cat offered a radically different model: persistent information, direct navigation by content, and an interface intended to disappear as the user developed muscle memory.

Carta Space does not attempt to reproduce the Canon Cat as a historical object. It asks what the same line of thought could produce with modern storage, Unicode text, version control, open document formats, and extensible software.

The proposed system is deliberately conservative in its implementation. Documents are UTF-8 CommonMark. Metadata is JSON. Identity uses UUIDv7. History uses Git. Portable archives use a ZIP-based container. Larger works are expressed as ordered references to documents, borrowing the successful separation between resources and reading order used by formats such as EPUB.

The novelty is therefore not a collection of new low-level technologies. It is the model produced by combining mature technologies under a deliberately small user-facing abstraction.

---

## 1. The problem

Most contemporary document software starts from the file.

The user creates a file, chooses a name, chooses a directory, saves it, later remembers or searches for that location, opens it with an application, and frequently encounters another layer of organizational machinery inside the application.

This arrangement is technically understandable. It is not obviously the most humane model for personal information.

Human memory often works differently. A person may remember:

- a phrase that appeared in a text;
- roughly when something was written;
- the larger work to which it belongs;
- the surrounding context;
- what changed during a particular period.

The filesystem answers a different question: *where was this object stored?*

Carta Space treats physical storage as an implementation matter and attempts to expose structures closer to the information itself.

---

## 2. Inspiration from Jef Raskin

The Canon Cat treated text as a persistent information space rather than as a collection of conventional files opened by separate applications. Its LEAP mechanism allowed rapid incremental navigation based on remembered content. The machine also minimized explicit save operations and attempted to make commands predictable and transient.

Carta Space adopts several principles from this tradition:

1. content should be primary;
2. navigation by remembered text should be extremely fast;
3. persistence should be automatic;
4. modes and administrative operations should be minimized;
5. structural concepts should correspond to meaningful properties of the information rather than to arbitrary storage locations.

Carta Space does **not** attempt to preserve the technical limitations of the 1987 machine. In particular, a modern system can search across many years of data, retain complete history, generate multiple publication formats, and represent a long intellectual work independently of physical storage units.

---

## 3. The three core objects

Carta Space distinguishes three concepts that conventional file-based workflows frequently collapse into one.

### 3.1 Document

A Document is the smallest persistent textual object in the core format.

It may be:

- a brief note;
- a letter;
- an article;
- a chapter;
- a preface;
- any other coherent piece of text.

Every Document has a permanent UUIDv7 identifier and an immutable creation timestamp. Its body is plain UTF-8 CommonMark.

A Document does not need a filename chosen by the user. Its title, when it has one, is content and is normally represented by a Markdown heading.

### 3.2 Volume

A Volume is an automatic chronological partition based on the creation month.

Documents created in September 2026 belong to the September 2026 Volume. Documents created in October belong to October.

The user does not decide when to open a new Volume, how large it should be, or what category it represents. The calendar makes the decision.

A Document never changes its original Volume merely because it is edited later.

The purpose of Volumes is not to classify knowledge. Their purpose is to provide a modest, predictable chronological information space analogous in cognitive scale to the separate media of earlier systems, without retaining their physical limitations.

### 3.3 Work

A Work is an ordered sequence of references to Documents.

This is the mechanism for long-form writing.

A novel may have its first chapter created in September, its second in October, and its introduction rewritten the following February. Their physical chronological placement does not matter. A Work records their intellectual order.

No content is copied into a Work. A Work references Documents by stable identifier.

This gives Carta Space two independent axes of organization:

- **time**, represented by Volumes;
- **intellectual structure**, represented by Works.

The distinction is fundamental.

---

## 4. Two principal views

Carta Space is intended to present the same information through different views rather than by duplicating it.

### 4.1 Chronological View

The Chronological View presents the Documents created in a selected period as a continuous readable space.

A monthly Volume is the natural default scope. Boundaries between Documents are visible but are generated by the interface rather than stored as editable characters.

This view answers questions such as:

- What did I create in May?
- What was I writing around that period?
- Where is the note I remember composing last autumn?

### 4.2 Work View

The Work View resolves the ordered list of Document identifiers belonging to a Work and presents their contents as one continuous intellectual object.

Document boundaries are structural and protected. The user can edit text on either side of a boundary, but cannot accidentally erase the boundary with Backspace or by selecting across it.

Operations such as splitting, merging, inserting, removing, or reordering component Documents are explicit structural commands.

A Work therefore behaves like a continuous book without pretending that all of its content is physically one large file.

---

## 5. Navigation and LEAP

Carta Space takes content-based navigation seriously.

The reference interface is expected to provide two LEAP operations:

- LEAP backward;
- LEAP forward.

In the initial Rust terminal prototype, the preferred experiment is to use the physical left and right Control keys as the two momentary LEAP controls. The terminal frontend should use a modern keyboard protocol capable of preserving physical-key identity and press/release events, allowing LEAP to behave as a true quasimode: hold a LEAP key, type the search string, release the key, and immediately return to ordinary editing.

This mapping is an implementation profile, not a storage-format rule. Carta Space defines LEAP backward and LEAP forward as interaction concepts; individual frontends may bind them differently when platform constraints require it.

LEAP should operate within an explicit scope. Likely scopes include:

- the current visible view;
- the current Work;
- the complete archive.

Time is also a retrieval aid. Because each Document has a stable creation timestamp and every modification is historically recorded, Carta Space can present temporal subsets such as "created in May" or "modified in May" and allow normal LEAP inside the resulting view.

The design deliberately postpones tags, semantic search, graph navigation, and AI-assisted retrieval. They should be introduced only if real use demonstrates that content search plus temporal context plus Works is insufficient.

---

## 6. Semantic writing instead of page design

Carta Space separates authorship from typesetting.

During composition, the author should express semantics:

- heading;
- emphasis;
- strong emphasis;
- quotation;
- list;
- code;
- link;
- paragraph structure.

The author should not normally be asked to choose fonts, point sizes, physical margins, or exact page geometry.

The initial representation is CommonMark 0.31.2. It is sufficiently expressive for ordinary structured prose while remaining readable as plain text.

Rendering is a later concern.

A Work or Document may eventually be rendered to:

- PDF;
- LaTeX;
- EPUB;
- HTML;
- DOCX or another interchange format.

A renderer or publication profile can decide that a chapter heading is set in a particular typeface and size. That decision does not alter the authored semantics.

This model intentionally resembles the separation of content and presentation found in systems such as LaTeX more than the direct-formatting model of desktop word processors.

---

## 7. History as an invisible capability

Carta Space should never require the ordinary user to perform manual saves in order to preserve work.

The current working state is written continuously or after very short idle periods.

Separately, the archive maintains historical checkpoints using Git.

Git is chosen because it already provides:

- content-addressed storage;
- efficient history for text;
- robust recovery;
- portable implementations;
- mature tooling;
- cryptographic object identity;
- historical diffs;
- an established ecosystem.

Carta Space does not expose Git as the normal user interface.

The distinction is:

- **autosave** protects the immediate present;
- **Git history** preserves the past.

A user may later ask to see a Document as it existed on a particular date without having created a named version manually.

A destructive purge operation must also exist. Ordinary deletion may remain historically recoverable. Purge removes the Document from the current archive, from structural references, and from reachable Git history. Such a purge cannot affect external backups, exported packages, or other copies.

---

## 8. Why the format uses ordinary files

A central resilience goal is that the archive remain intelligible even if Carta Space software disappears.

The current text of every Document is an ordinary Markdown file. Administrative metadata is JSON. Works are JSON documents containing ordered identifiers. History is an ordinary Git repository.

This rejects an opaque database as the canonical representation.

Databases, search indexes, embeddings, or caches may later exist as derived acceleration layers. They must not become the sole authoritative copy of authored content.

The preferred failure mode is graceful degradation:

1. Carta Space disappears;
2. the user opens the archive with generic tools;
3. the Markdown is still readable;
4. the JSON is still inspectable;
5. Git can still expose history.

---

## 9. Extensibility without speculative complexity

Carta Space is intended to evolve, but the initial format must not contain unused machinery for imagined future features.

Possible future capabilities include:

- images and other assets;
- bibliographic data and citations;
- tags;
- footnotes beyond the CommonMark core;
- richer Work structures;
- cross-document relationships;
- derived full-text indexes;
- semantic search;
- synchronization;
- collaboration;
- AI-assisted operations.

Version 0.1 implements none of these merely because they might someday be useful.

Instead, the format establishes a small number of extension rules:

- the archive format is explicitly versioned;
- identifiers are stable;
- unknown JSON members must be preserved when rewriting an object;
- unknown files and directories must not be destroyed merely because a frontend does not understand them;
- new features should preferably be additive;
- derived data must be distinguishable from canonical authored data.

The design rule is:

> Complexity must be earned by a demonstrated problem.

---

## 10. Single-user first

The initial system is deliberately local, single-user, and single-writer.

Concurrent editing introduces distributed-state problems: conflict resolution, merging, causality, synchronization, network failure, and identity. Those are legitimate problems, but they are not required to test Carta Space's central hypothesis.

A successful local system can later acquire synchronization. A complicated synchronized system that never proves the underlying writing model would be wasted effort.

---

## 11. Portable `.cat` packages

The working archive is an ordinary directory tree and Git repository.

For interchange, archival, or transfer, Carta Space defines a portable `.cat` package. It is ZIP-based, following the successful general pattern used by formats such as EPUB and ODF.

The package contains the archive tree and its history. A small `mimetype` member identifies the container.

The `.cat` package is not intended to be rewritten on every keystroke. It is produced atomically from the working archive when exporting or packaging.

This keeps the working format friendly to editors and Git while giving users a single portable object when they need one.

---

## 12. What must be tested experimentally

Several aspects of Carta Space are hypotheses, not conclusions.

### 12.1 LEAP at scale

LEAP worked in an environment with far less information than a modern archive can hold. Time filtering, Work scope, and archive scope may be sufficient to scale it. They may not.

This must be learned from use rather than solved pre-emptively with tags or semantic machinery.

### 12.2 Document granularity

Carta Space does not prescribe that every paragraph become a Document or that every month become one Document. Users explicitly create meaningful boundaries.

Real use will reveal whether this remains comfortable.

### 12.3 Continuous Work editing

Presenting many physical Markdown files as one editable Work requires careful handling of protected structural boundaries. The model is simple; the user interface is not trivial.

The first implementation should prefer structural safety over cleverness.

---

## 13. Design objective

Carta Space should feel smaller than the technology underneath it.

Internally the reference implementation may use Rust libraries, Git, Markdown parsers, terminal protocols, renderers, and conversion tools. None of those should become conceptual burdens for ordinary writing.

The desired mental model is approximately:

1. write;
2. search;
3. move through time;
4. open a Work;
5. publish when necessary.

If the user routinely needs to think about UUIDs, paths, Git commits, JSON, ZIP files, or renderer configuration, the implementation has exposed too much of itself.

Carta Space succeeds only if its internal sophistication produces external simplicity.

---

## 14. Reference implementation architecture

Carta Space deliberately separates the information model from its interactive presentation.

The planned reference implementation uses Rust and is divided into layers:

- **carta-format** defines the format-facing data model, serialization, validation, and compatibility behavior;
- **carta-core** implements archive semantics such as Document creation, monthly Volumes, Works, search, LEAP logic, Git-backed history, purge, and portable-package operations;
- **carta-cli** exposes scriptable and administrative operations in a conventional Unix command-line form;
- **carta-tui** provides the first full-screen interactive writing environment using Ratatui and Crossterm.

The TUI MUST NOT become the sole place where archive semantics live. Interactive frontends should call the core rather than directly editing archive structure behind its back.

This architecture serves two purposes. First, most Carta Space behavior can be tested without a graphical or terminal interface. Second, alternative frontends can be developed later without changing the format or reimplementing the difficult parts of the system.

A graphical frontend is therefore a possible future implementation, not a requirement. An Emacs frontend is likewise possible, but Emacs is no longer the planned reference implementation.

The terminal is preferred initially because Carta Space is keyboard-centered, text-centered, and intentionally non-WYSIWYG. Publication-quality appearance belongs to renderers such as LaTeX, HTML, or EPUB rather than to the composition interface itself.

The principal terminal-specific risk is keyboard fidelity. A traditional terminal often loses the distinction between physical modifier keys and does not report key release events. The reference TUI should therefore detect and use enhanced keyboard reporting where available and provide an explicit fallback when it is not.
