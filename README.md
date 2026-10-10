# Carta Space

> **Current release:** v0.1.2  
> **Format draft:** 0.1  
> **Platforms:** Linux and macOS  
> **License:** GPL-3.0-or-later (software); CC0-1.0 (original project documentation).
>
> **Interface status:** v0.1.2 is the final terminal release. The development branch is now v0.2, centered on a shared application layer and a graphical desktop frontend.

Carta Space is an experimental document environment inspired by Jef Raskin's work on the Canon Cat and later humane-interface research.

Its central idea is simple: the user should primarily deal with **content**, not with filenames, directories, save dialogs, or application boundaries.

Carta Space is deliberately **writing-first**. Organization, retrieval, history, and publication should support writing without becoming prerequisites for it. Creating text should not require the user to choose a filename, directory, tag, template, knowledge-management method, or presentation style first.

This is a form of **distraction-free architecture**, not merely a visually minimal interface. Carta Space may eventually provide powerful Views, backlinks, history, publishing tools, and optional semantic retrieval, but such capabilities should appear when requested rather than permanently compete with the text for attention.

Carta Space stores documents as ordinary UTF-8 Markdown, gives every document a stable identity, arranges newly created documents in automatic monthly volumes, and allows documents to be assembled into larger **Works** without copying or moving them. A Work can therefore span months or years while remaining editable as a single continuous view.

The reference implementation is written in Rust and is explicitly split into frontend-independent format/core/application layers, a Unix-style command-line interface, and presentation adapters. The frozen terminal frontend is the first adapter; v0.2 development targets a graphical desktop frontend. The format remains independent of Rust and of any particular frontend.

## Install and run

Carta Space supports Linux and macOS. Nix is one installation option, not a requirement. The main installation paths are:

- **prebuilt Linux x86_64 binary** — built on Ubuntu 20.04 for a glibc 2.31 baseline;
- **prebuilt macOS binaries** — separate builds for Apple Silicon and Intel;
- **build from source** — the broadest route across Linux distributions, architectures, and macOS;
- **Nix / NixOS** — native flake packaging, without `steam-run` or `nix-ld`.

Git is a runtime dependency because Carta archive history and synchronization are Git-backed. PDF publication additionally requires Typst 0.15.1 or newer at runtime; optional spelling requires Hunspell and language dictionaries. The v0.2 Nix editor packages provide Git, Typst, Hunspell and the dictionaries automatically, without installing these tools globally.

### Prebuilt Linux x86_64

Download the v0.1.2 release and its checksum:

```bash
wget https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-linux-x86_64.tar.gz
wget https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-linux-x86_64.tar.gz.sha256
sha256sum -c carta-v0.1.2-linux-x86_64.tar.gz.sha256

tar -xzf carta-v0.1.2-linux-x86_64.tar.gz
cd carta-v0.1.2-linux-x86_64

install -Dm755 carta ~/.local/bin/carta
install -Dm755 carta-cli ~/.local/bin/carta-cli   # optional
carta
```

The v0.1.2 Linux binary is built and tested on Ubuntu 20.04 with glibc 2.31 and Git 2.25.1. It is intended for x86_64 distributions with glibc 2.31 or newer. If it is incompatible with your distribution, use the source-build instructions below instead.

### Prebuilt macOS

Choose the archive for your Mac.

Apple Silicon:

```bash
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-arm64.tar.gz
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-arm64.tar.gz.sha256
shasum -a 256 -c carta-v0.1.2-macos-arm64.tar.gz.sha256
tar -xzf carta-v0.1.2-macos-arm64.tar.gz
cd carta-v0.1.2-macos-arm64
```

Intel:

```bash
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-x86_64.tar.gz
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-x86_64.tar.gz.sha256
shasum -a 256 -c carta-v0.1.2-macos-x86_64.tar.gz.sha256
tar -xzf carta-v0.1.2-macos-x86_64.tar.gz
cd carta-v0.1.2-macos-x86_64
```

Then install:

```bash
mkdir -p ~/.local/bin
install -m 755 carta ~/.local/bin/carta
install -m 755 carta-cli ~/.local/bin/carta-cli   # optional
carta
```

The macOS binaries use a macOS 11 deployment target and are CI-tested on Apple Silicon and Intel. They are currently not Apple Developer signed or notarized; if macOS blocks a quarantined download, use the source-build method below.

### Build from source

Building locally is the distribution-independent fallback. It also supports Linux architectures other than x86_64 when the Rust dependencies support the target.

On Linux you need Rust 1.85 or newer, Cargo, Git, a C build toolchain, pkg-config/pkgconf, and Wayland development files. On macOS you need Rust 1.85 or newer, Git, and the Xcode Command Line Tools. Then:

```bash
git clone --depth 1 --branch v0.1.2 https://github.com/fnt400/carta-space.git
cd carta-space

cargo install --locked --path crates/carta-tui
cargo install --locked --path crates/carta-cli   # optional

carta
```

See [INSTALL.md](INSTALL.md) for dependency commands for Debian/Ubuntu, Fedora, Arch/Manjaro, Alpine, and guidance for other distributions.

### Nix / NixOS

Run Carta directly:

```bash
nix run github:fnt400/carta-space
```

Or install it into the current Nix profile:

```bash
nix profile install github:fnt400/carta-space
carta
```

The Nix package builds Carta natively. The v0.2 GUI and terminal packages provide Git, Typst and dictionary-enabled Hunspell inside their own wrappers (Italian, French, UK/US English, German and Spanish); no separate NixOS system packages are needed.

On first launch, `carta` opens the default XDG archive location and offers to create an empty Archive or import an existing Git Archive when none exists.

#### Experimental graphical v0.2 branch

On `opencode/v0.2`, the flake builds the GUI by default. From the
repository checkout:

```bash
nix build
./result/bin/carta-gui
```

The GUI wrapper adds Git, Typst and Hunspell with dictionaries to its runtime
`PATH`. It uses Nixpkgs' `hunspellWithDicts` (including `DICPATH`) so the
spell-checker's locale codes such as `it_IT`, `fr_FR` and `en_GB` work without
installing those packages on the host. PDF export uses the same bundled Typst.

A quick isolated spelling test (without opening your real Archive):

```bash
TEST_ARCHIVE="$(mktemp -d)/archive"
nix run .#carta-cli -- create "$TEST_ARCHIVE"
./result/bin/carta-gui "$TEST_ARCHIVE"
```

Enter a sentence with a typo, choose `Set Document Language…` in the palette,
then `Check Document Spelling`. To test PDF publishing, use
`Export Document PDF` on the same disposable Archive.
When running `cargo run` inside Distrobox instead of a Nix wrapper, install
Hunspell and dictionaries **inside the Distrobox**; Nix's wrapped tools
are not injected into that environment.

The GUI uses the existing Archive under
`${XDG_DATA_HOME:-$HOME/.local/share}/carta/archive when launched without
arguments; to use another Archive, provide its path explicitly:

```bash
./result/bin/carta-gui /path/to/archive
```

The other programs remain available:

```bash
nix build .#carta-space  # TUI + CLI
nix run .#carta          # TUI
nix run .#carta-cli      # CLI
```

To **install** the graphical application and its XDG `.desktop` launcher into
your Nix user profile, run:

```bash
nix profile install .#carta-gui
```

The launcher appears as **Carta Space** in desktop application menus where
Nix profile desktop entries are indexed. Building with `nix build` alone
only creates the `result` symlink; it does not register a launcher. The
initial GUI still requires an existing Archive, which can be created with
the reference TUI or CLI. It uses the bundled Iosevka font and a generic
text-editor icon in the desktop menu.

The GUI is a separate Cargo project with its own committed
`crates/carta-gui/Cargo.lock` to make Nix dependency resolution reproducible.
Its Rust minimum version is 1.88. The terminal workspace and v0.1.x
compatibility build remain separate.

The GUI automatically synchronizes an Archive with its dedicated `carta-sync`
remote, or adopts an existing `origin/carta` upstream when no dedicated remote
is configured. Archives without either remain local-only. `Sync Settings`
can configure or explicitly disable synchronization; disabling persists across
restarts. Carta does not encrypt synchronization: anyone who can read the remote
can read the authored text, metadata and retained Git history.

Committed checkpoints are published automatically in the background, including
already-unpublished checkpoints found at startup. Pulls are staged privately
and integrated only when the editor is safely idle and the Archive has not
changed. All fetch/push destinations are validated against the Archive identity;
jobs pin their destinations even if Git configuration changes during transfer.

Setup, network and conflict errors appear as nonblocking status warnings.
Writing and local checkpoints continue; failed transfers remain queued and
retry with backoff. Both palette Quit and closing the desktop window save and
checkpoint locally, then exit without waiting for Git/SSH. The final checkpoint
may therefore remain local until the next startup. Git/SSH operations currently
have no enforced completion deadline, but do not hold the GUI open on Quit.
Publication to multiple destinations is not atomic; successful earlier pushes
are retained and remaining destinations are retried safely without force-push.

After adopting origin, Carta uses the dedicated `carta-sync` tracking refs.
`git status` may still compare against an outdated `origin/carta`; fetching
origin refreshes that comparison. Publication tests verify the remote branch's
actual commit, not just a local tracking ref.

The helper programs under `scripts/` are development and maintenance tools. They are not part of the normal installation or startup path.

## Reference implementation architecture

The Rust workspace is split so that Carta behavior is not owned by one presentation layer:

- `carta-format` — format types, validation, serialization, and compatibility rules;
- `carta-core` — canonical Archive/domain operations: Documents, Volumes, Works, retrieval/LEAP, history, synchronization, Trash/Wipe and backlinks;
- `carta-app` — reusable interactive application layer containing the editor, Cat highlight/undo engine, session/View/AppMode models, scheduler, palette matching, semantic `Action` and `ModeAction` boundaries, shared help content, and the canonical `App` controller;
- `carta-publish` — frontend-independent publication layer with a replaceable PDF backend; the first backend uses Typst;
- `carta-cli` — scriptable Unix-style administrative commands;
- `carta-tui` — the frozen v0.1.x terminal adapter, retained as a compatibility frontend and regression oracle; it consumes `carta-app` rather than owning a second application controller;
- `carta-gui` — Iced 0.14 graphical desktop frontend for v0.2. A dedicated input probe has validated physical Left/Right Ctrl/Alt press/release, normal Unicode input and AltGr behavior; the initial GUI shell is being developed separately from the frozen v0.1 compatibility workspace.

The architectural rule is one Carta behavior with multiple possible surfaces. Desktop, terminal, web and mobile frontends should translate native events into shared application semantics and render shared state rather than reimplementing the editor.

The TUI demonstrated that enhanced terminal protocols can support the intended LEAP gesture in some terminals, but real Linux TTYs cannot portably expose the required physical modifier identity and release events without system-specific input access. For that reason v0.1.2 closes the terminal-first line. Portable Keyboard Mode remains a legacy compatibility mechanism, not the interaction model for v0.2.

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

Built-in PDF publication uses the fixed Carta Classic profile through the isolated `carta-publish` layer. The current backend invokes Typst and supplies Carta's own embedded fonts; Markdown export remains available as the renderer-independent interchange path.

## Command line

The optional `carta-cli` binary uses `--archive <path>` and has a noninteractive, scriptable command surface. Without `--archive`, it opens `$XDG_DATA_HOME/carta/archive` (or `~/.local/share/carta/archive` when `XDG_DATA_HOME` is unset):

```text
carta-cli create <path>
carta-cli --archive <path> validate|inspect|documents|works|history
carta-cli --archive <path> checkpoint [--kind manual] [--note <text>]
carta-cli --archive <path> import <file>
carta-cli --archive <path> search <literal-query>
carta-cli --archive <path> export markdown-document <document-id>
carta-cli --archive <path> export markdown-work <work-id>
carta-cli --archive <path> package <output.cat>
carta-cli --archive <path> trash inventory|impact|document|work ...
carta-cli --archive <path> restore document|work ...
carta-cli --archive <path> wipe execute <document-id> --confirm "WIPE <document-id> PERMANENTLY"
```

Markdown exports are written exactly to standard output. Package destinations must be outside the Archive. Trash mutations require `--confirm`; `wipe execute --confirm` performs a fresh preflight and requires the exact strong confirmation phrase without an interactive prompt. Run `carta-cli <command> --help` for complete arguments.

## Terminal interface

`carta` is the writing-first full-screen frontend and the normal user entry point. Pass an Archive directory explicitly, create one with `--create`, or omit the path to open the deterministic XDG default Archive at `$XDG_DATA_HOME/carta/archive` (or `~/.local/share/carta/archive`):

```bash
carta /path/to/archive
carta --create /path/to/new-archive
carta
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
- `LICENSES.md` — licensing scope for software and documentation.

## Current scope

The current Rust workspace contains `carta-format`, `carta-core`, `carta-app`, `carta-publish`, `carta-cli`, and the frozen `carta-tui`. The first usable v0.1 can create, read, validate, inspect, search, import, export, package, edit, navigate, recover, and administer Draft 0.1 Archives. It implements the Reader and Writer responsibilities, the accepted v0.1 interaction contract, and the explicitly scoped on-device Wipe guarantee. Draft 0.1 remains experimental and is not a stable 1.0 format.

The enhanced-keyboard-reporting experiment remains separate under `experiments/keyboard-events` as a diagnostic for terminal compatibility.

Development of v0.2 (including the experimental GUI) uses the Ubuntu 26.04
Distrobox `carta-gui-dev` described in `AGENTS.md`, with Rust >= 1.88.
From the repository root on the NixOS host, run:

```bash
distrobox enter carta-gui-dev
bash scripts/verify-carta.sh
```

The verification script checks the normal Rust workspace and the standalone GUI
crate, including the optimized GUI release build. It does **not** establish
runtime smoothness: sustained GUI responsiveness must be tested manually with a
representative Archive. The old Debian `carta-dev` is retained only for
legacy v0.1.2 TUI compatibility as needed.

The v0.1 interaction contract now fixes startup/resume behavior, LEAP semantics, the contextual dmenu-like command palette, editing and clipboard behavior, Creation Date/Modification Date/Work/Search Views, Work operations, links/backlinks, History, Trash/Wipe, import/export, autosave/checkpoints, Git synchronization, recovery, and structural atomicity. Implementation work should follow `INTERACTION-CONTRACT.md` rather than inventing missing UI semantics.

Features such as images, bibliographies, tags, semantic search, collaboration, AI assistance, and interactive synchronization-conflict resolution remain intentionally deferred until real use demonstrates a need for them.

The working rule is:

> **Complexity must be earned by a demonstrated problem.**


## Optional spelling (v0.2)

Carta checks spelling only when requested. Each Document saves an optional
language in its canonical metadata, and Work spelling respects each member's
language. Choose **Set Document Language…** or **Check Document Spelling**.

**Dictionary installation is automatic and device-local.** If Hunspell already
has a system dictionary for the chosen language, Carta reuses it. Otherwise a
background worker downloads only the required dictionary and its original
license over HTTPS. Every file comes from a pinned revision of
`wooorm/dictionaries` and is verified against its Git object ID before an
atomic cache publication. Carta never downloads anything at startup, and an
error does not prevent writing. After the initial download, the dictionary
works offline. The pending spelling review resumes automatically once all
required languages are ready.

The download uses the cross-platform `curl` command and verifies the source
files with `git hash-object`. The Hunspell executable is still necessary:
this implementation downloads **dictionaries**, not executable binaries.
The Nix Carta wrappers supply Git, curl and Hunspell themselves, with the
common languages preinstalled. Distrobox development setup supplies Git,
curl and Hunspell without preinstalling dictionaries. Other distribution
packages must provide these three tools at runtime.

The supported automatic downloads are `it_IT`, `fr_FR`, `en_GB`,
`en_US`, `de_DE` and `es_ES`. The cache is outside the Archive at
`$XDG_DATA_HOME/carta/dictionaries`, falling back to
`~/.local/share/carta/dictionaries` on Unix or the local application data
directory on Windows. It is never committed to Git or included in `.cat`.

Personal words remain in `Archive/spelling/<language>.dic`, synchronized
through ordinary Git checkpoints. The general dictionary, the personal list,
and the per-Document language are distinct. Each downloaded dictionary's
`LICENSE.txt` retains the upstream licensing terms; see the source
catalog in `crates/carta-app/src/dictionary_manager.rs`.
