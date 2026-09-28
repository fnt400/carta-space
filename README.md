# Carta Space

> **Status:** Working name; early design draft.  
> **Current format draft:** 0.1  
> **Reference implementation:** first usable Rust v0.1.

Carta Space is an experimental document environment inspired by Jef Raskin's work on the Canon Cat and later humane-interface research.

Its central idea is simple: the user should primarily deal with **content**, not with filenames, directories, save dialogs, or application boundaries.

Carta Space is deliberately **writing-first**. Organization, retrieval, history, and publication should support writing without becoming prerequisites for it. Creating text should not require the user to choose a filename, directory, tag, template, knowledge-management method, or presentation style first.

This is a form of **distraction-free architecture**, not merely a visually minimal interface. Carta Space may eventually provide powerful Views, backlinks, history, publishing tools, and optional semantic retrieval, but such capabilities should appear when requested rather than permanently compete with the text for attention.

Carta Space stores documents as ordinary UTF-8 Markdown, gives every document a stable identity, arranges newly created documents in automatic monthly volumes, and allows documents to be assembled into larger **Works** without copying or moving them. A Work can therefore span months or years while remaining editable as a single continuous view.

The reference implementation is written in Rust and is explicitly split into a frontend-independent core, a Unix-style command-line interface, and a terminal user interface. The format remains independent of Rust and of any particular frontend.

## Reference implementation architecture

The initial implementation is planned as a small set of separable Rust components:

- `carta-format` — format types, validation, serialization, and compatibility rules;
- `carta-core` — archive operations, Documents, Volumes, Works, LEAP search, history, Trash/Wipe, import/export;
- `carta-cli` — scriptable Unix-style administrative commands, producing the `carta` binary;
- `carta-tui` — the first interactive frontend, built with Ratatui and Crossterm.

The TUI is the reference interactive environment, not the definition of Carta Space. Future GTK, Emacs, web, or other frontends should use the same core model rather than reimplementing archive semantics.

The reference terminal environment successfully distinguished physical left and right Control and reported separate press/release events through Crossterm enhanced keyboard reporting. The TUI uses them as the initial experimental bindings for the two momentary LEAP controls. Terminals without enhanced reporting use the approved palette LEAP commands instead. The key mapping is an implementation choice, not part of the archive format.

## Interaction philosophy

Carta Space gives priority to **writing and content production** over organization and presentation.

The intended default interaction is therefore deliberately quiet:

- starting a new Document should require as little administrative input as possible;
- organizational and retrieval tools should be available on demand rather than permanently displayed;
- commands should prefer transient or quasimodal interaction when that makes returning to the text immediate;
- publishing and visual formatting belong after authorship, not inside the normal writing loop;
- a simple dedicated distraction-free writer, a richer desktop frontend, and a lightweight capture frontend may all expose the same Archive through different interfaces.

The goal is not to make the system weak. The goal is to keep its power out of the user's way until that power is needed.

## Core principles

Carta Space 0.x is deliberately small.

- Writing takes precedence over organization and presentation.
- Power should be available on demand, not presented continuously.
- Capture should not require prior classification.
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
- an organization-first personal knowledge-management system;
- an "everything app" for tasks, CRM, calendars, and project management;
- an AI-first application.

Some of those capabilities might later be implemented as extensions, but none belongs to the initial core.

## Documents, Volumes, Works

A **Document** is the basic persistent unit. It may be a short note, a letter, an essay, or a chapter. Its creation time is immutable; Carta also records the last successful authored-content modification, with creation time as the fallback for Documents that have never been edited.

A **Volume** is an automatic chronological partition. A document created in September 2026 belongs permanently to the `2026/09` volume. Editing it later does not move it.

A **Work** is an intellectual structure: an ordered sequence of references to Documents. A novel can therefore contain chapters created in many different monthly volumes while appearing to the author as one continuous work.

This deliberately separates two questions:

- *When did this document enter my archive?*
- *What larger work does this document belong to?*

The answers need not be the same kind of structure.

## History

A Carta Space archive is also a Git repository. Git is an implementation-independent part of the archive format, not a user-facing workflow. Normal users should not need to understand commits, staging, branches, or merges.

Current content remains readable without Git. Git provides automatic historical reconstruction.

## TUI Markdown highlighting

The TUI shows canonical CommonMark source literally while applying syntax colors to headings, emphasis, strong emphasis, block quotations, inline/fenced code, links, and images. Default colors use the terminal ANSI palette rather than fixed RGB values.

The Markdown palette can be overridden with `CARTA_MD_HEADING_COLOR`, `CARTA_MD_EMPHASIS_COLOR`, `CARTA_MD_STRONG_COLOR`, `CARTA_MD_QUOTE_COLOR`, `CARTA_MD_CODE_COLOR`, and `CARTA_MD_LINK_COLOR`. Values may be ANSI color names such as `light-blue` or `cyan`, or `#RRGGBB`.

## Trash, Restore, and Wipe

`carta-core` implements recoverable Trash without a canonical `trash/` directory. The Trash view is derived from current state and retained Git history. Trashing a Document reports every current Work membership and inbound Carta link, removes the structural Work references and active Document together, leaves authored Markdown links unchanged, and creates an immediate structural checkpoint. Documents restore with their original identity, creation time, Volume, and content immediately before Trash, but without former Work memberships. Works restore with their original identity and order; restoring required trashed Documents needs explicit consent, and an active title conflict requires an explicit unique replacement title.

Permanent Wipe is Document-only and requires exact strong confirmation. Execution performs a fresh preflight and refuses active Documents, tracked working changes, additional Git worktrees, unhandled pseudorefs, and in-progress Git operations. It rewrites retained local Git refs containing the Document's canonical paths, expires reflogs, prunes unreachable exclusively owned objects, removes Carta-reserved `.carta-*`/`carta-*` staging, cache, session, and temporary artifacts, and verifies that the target identity and paths are gone. If identical content remains independently reachable through another Document, that shared blob is preserved and Carta does not claim byte erasure of the independent content.

The guarantee is deliberately scoped to Carta-controlled data in the current Archive on the current device. It does not cover external backups, exported packages, clones, filesystem snapshots, copied files, or physical storage remnants.

## Portability

The working form of a Carta Space archive is an ordinary directory tree containing Markdown and JSON plus `.git`.

A portable `.cat` package is a ZIP-based container of that tree. Its purpose is transfer, backup, and interchange; it is not the file continuously rewritten while the user types.

`carta-core` creates packages through a temporary sibling of the destination, validates the ZIP and its unpacked Archive before replacement, and checkpoints current state first. The exact uncompressed `mimetype` is the first member; canonical files, unknown resources, and complete Git history are included, while reserved Carta-managed temporary artifacts are omitted.

Built-in PDF publication is intentionally deferred beyond v0.1. Markdown export remains available for external publishing tools.

## Command line

The `carta` binary uses `--archive <path>` and has a noninteractive, scriptable command surface. Without `--archive`, it opens `$XDG_DATA_HOME/carta/archive` (or `~/.local/share/carta/archive` when `XDG_DATA_HOME` is unset):

```text
carta create <path>
carta --archive <path> validate|inspect|documents|works|history
carta --archive <path> checkpoint [--kind manual] [--note <text>]
carta --archive <path> import <file>
carta --archive <path> search <literal-query>
carta --archive <path> export markdown-document <document-id>
carta --archive <path> export markdown-work <work-id>
carta --archive <path> package <output.cat>
carta --archive <path> trash inventory|impact|document|work ...
carta --archive <path> restore document|work ...
carta --archive <path> wipe execute <document-id> --confirm "WIPE <document-id> PERMANENTLY"
```

Markdown exports are written exactly to standard output. Package destinations must be outside the Archive. Trash mutations require `--confirm`; `wipe execute --confirm` performs a fresh preflight and requires the exact strong confirmation phrase without an interactive prompt. Run `carta <command> --help` for complete arguments.

## Terminal interface

`carta-tui` is the writing-first full-screen frontend. Pass an Archive directory explicitly, create one with `--create`, or omit the path to open the deterministic XDG default Archive at `$XDG_DATA_HOME/carta/archive` (or `~/.local/share/carta/archive`):

```bash
carta-tui /path/to/archive
carta-tui --create /path/to/new-archive
carta-tui
```

Device-local session state is stored alongside the default Archive root under `$XDG_DATA_HOME/carta` (or `~/.local/share/carta`), but outside the `archive/` Git working tree; session files are named `session-<archive-id>.json`. On startup, the TUI migrates legacy session files from `$XDG_STATE_HOME/carta-space` (or `~/.local/state/carta-space`) and discards the obsolete `last-archive.json`.

If the default Archive does not exist, the TUI offers a minimal first-run choice: create an empty Archive, import an existing Git Archive, or quit. Git import asks for the repository URL and explicitly clones the reserved `carta` branch, so it does not depend on the remote repository's default branch or `HEAD`. The clone is validated in a temporary sibling directory before it replaces the missing default path, and the remote is configured as Carta's synchronization remote. Existing paths are never overwritten by this flow. A new Archive or an Archive without session state opens directly into a provisional empty Document in the current month.

The default surface is an editable, soft-wrapped monthly Creation Date View or continuous Work View. A separate Modification Date View spans all active Documents and orders them from least recently modified to most recently modified. Generated separator lines protect Document boundaries. `Esc` opens the contextual command palette. On capable terminals, physical Left Ctrl/Left Alt provide LEAP and Right Ctrl is the Carta command modifier: RightCtrl+Z/R handle undo/redo, RightCtrl+C performs Cat COPY or clipboard paste, RightCtrl+W opens the Work selector, and RightCtrl+L inserts or opens a link at point. Copy, Cut, and Paste otherwise remain terminal-emulator operations.

If canonical Document content or Work structure changes externally while the TUI has a divergent local edit, `carta-core` durably preserves both variants under `.git/carta-conflicts/` before refusing the write. The TUI opens a read-only Conflicts view where the user explicitly chooses the current local or external variant. For Documents, the non-selected variant may optionally be preserved as a new neutral Document; Work resolution never creates a second Work automatically. These records are device-local recovery state, are excluded from portable packages, and are scrubbed when their Document is Wiped.

## Documents in this repository

- `WHITEPAPER.md` — why Carta Space exists and the design philosophy.
- `SPECIFICATION.md` — normative draft of Carta Space Format 0.1.
- `INTERACTION-CONTRACT.md` — accepted behavior of the first usable v0.1 interaction model.
- `DESIGN-DECISIONS.md` — important architectural decisions and their rationale.
- `LICENSES.md` — proposed licensing model.

## Current scope

The current Rust workspace contains `carta-format`, `carta-core`, `carta-cli`, and `carta-tui`. The first usable v0.1 can create, read, validate, inspect, search, import, export, package, edit, navigate, recover, and administer Draft 0.1 Archives. It implements the Reader and Writer responsibilities, the accepted v0.1 interaction contract, and the explicitly scoped on-device Wipe guarantee. Draft 0.1 remains experimental and is not a stable 1.0 format.

The enhanced-keyboard-reporting experiment remains separate under `experiments/keyboard-events` as a diagnostic for terminal compatibility.

Development commands run in the Debian Distrobox described in `AGENTS.md`. The container requires Rust, Cargo, rustfmt, Clippy, and Git. From the repository root, validate the workspace with:

```bash
distrobox enter carta-dev
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

The v0.1 interaction contract now fixes startup/resume behavior, LEAP semantics, the contextual dmenu-like command palette, editing and clipboard behavior, Creation Date/Modification Date/Work/Search Views, Work operations, links/backlinks, History, Trash/Wipe, import/export, autosave/checkpoints, Git synchronization, recovery, and structural atomicity. Implementation work should follow `INTERACTION-CONTRACT.md` rather than inventing missing UI semantics.

Features such as images, bibliographies, tags, semantic search, collaboration, AI assistance, and interactive synchronization-conflict resolution remain intentionally deferred until real use demonstrates a need for them.

The working rule is:

> **Complexity must be earned by a demonstrated problem.**
