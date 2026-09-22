# Carta Space

> **Status:** Working name; early design draft.  
> **Current format draft:** 0.1  
> **Reference implementation:** not yet started.

Carta Space is an experimental document environment inspired by Jef Raskin's work on the Canon Cat and later humane-interface research.

Its central idea is simple: the user should primarily deal with **content**, not with filenames, directories, save dialogs, or application boundaries.

Carta Space stores documents as ordinary UTF-8 Markdown, gives every document a stable identity, arranges newly created documents in automatic monthly volumes, and allows documents to be assembled into larger **Works** without copying or moving them. A Work can therefore span months or years while remaining editable as a single continuous view.

The reference implementation is planned in Rust and is explicitly split into a frontend-independent core, a Unix-style command-line interface, and a terminal user interface. The format remains independent of Rust and of any particular frontend.

## Reference implementation architecture

The initial implementation is planned as a small set of separable Rust components:

- `carta-format` — format types, validation, serialization, and compatibility rules;
- `carta-core` — archive operations, Documents, Volumes, Works, LEAP search, history, purge, import/export;
- `carta-cli` — scriptable Unix-style administrative commands;
- `carta-tui` — the first interactive frontend, built with Ratatui and Crossterm.

The TUI is the reference interactive environment, not the definition of Carta Space. Future GTK, Emacs, web, or other frontends should use the same core model rather than reimplementing archive semantics.

The reference TUI will experimentally map the two momentary LEAP controls to the physical left and right Control keys. A modern terminal keyboard protocol capable of distinguishing physical modifier keys and key press/release events is therefore preferred. The key mapping is an implementation choice, not part of the archive format.

## Core principles

Carta Space 0.x is deliberately small.

- Documents are persistent objects with stable UUIDv7 identities.
- Document bodies are UTF-8 CommonMark 0.31.2.
- Administrative metadata is stored separately as JSON.
- Documents are physically grouped by creation month.
- A Work is an ordered list of document identifiers.
- Work boundaries are structural and cannot be deleted as ordinary text.
- Search and navigation are centered on a LEAP-style incremental mechanism.
- The archive keeps automatic history using Git.
- Presentation is semantic-first: authors describe what text *is*, not how many points or millimetres it should occupy.
- Printing and publishing are separate rendering operations.
- The format is designed for additive, backward-compatible extension.
- The initial implementation is local, single-user, and single-writer.

## What Carta Space is not

Carta Space is not intended to be:

- a clone of Microsoft Word;
- a filesystem replacement;
- a knowledge graph;
- a tag-management system;
- a collaborative editor;
- a note-taking database;
- an AI-first application.

Some of those capabilities might later be implemented as extensions, but none belongs to the initial core.

## Documents, Volumes, Works

A **Document** is the basic persistent unit. It may be a short note, a letter, an essay, or a chapter.

A **Volume** is an automatic chronological partition. A document created in September 2026 belongs permanently to the `2026/09` volume. Editing it later does not move it.

A **Work** is an intellectual structure: an ordered sequence of references to Documents. A novel can therefore contain chapters created in many different monthly volumes while appearing to the author as one continuous work.

This deliberately separates two questions:

- *When did this document enter my archive?*
- *What larger work does this document belong to?*

The answers need not be the same kind of structure.

## History

A Carta Space archive is also a Git repository. Git is an implementation-independent part of the archive format, not a user-facing workflow. Normal users should not need to understand commits, staging, branches, or merges.

Current content remains readable without Git. Git provides automatic historical reconstruction.

## Portability

The working form of a Carta Space archive is an ordinary directory tree containing Markdown and JSON plus `.git`.

A portable `.cat` package is a ZIP-based container of that tree. Its purpose is transfer, backup, and interchange; it is not the file continuously rewritten while the user types.

## Documents in this repository

- `WHITEPAPER.md` — why Carta Space exists and the design philosophy.
- `SPECIFICATION.md` — normative draft of Carta Space Format 0.1.
- `DESIGN-DECISIONS.md` — important architectural decisions and their rationale.
- `LICENSES.md` — proposed licensing model.

## Current scope

The next step after this documentation is a minimal Rust prototype: first the format/core and keyboard-event experiment, then a deliberately small terminal frontend. Features such as images, bibliographies, tags, semantic search, synchronization, collaboration, and AI assistance are intentionally deferred until real use demonstrates a need for them.

The working rule is:

> **Complexity must be earned by a demonstrated problem.**
