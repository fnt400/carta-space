# Carta Space — Agent Instructions

Carta Space is a content-first document environment inspired by Jef Raskin's Canon Cat and humane-interface work.

Before making architectural or implementation changes, read:

1. `SPECIFICATION.md` — normative definition of the Carta Space format.
2. `INTERACTION-CONTRACT.md` — accepted v0.1 user-visible behavior for the reference interaction model.
3. `DESIGN-DECISIONS.md` — accepted architectural decisions and rationale.
4. `WHITEPAPER.md` — project philosophy and conceptual model.
5. `README.md` — current project overview.

If these documents appear to conflict, `SPECIFICATION.md` is authoritative for the archive format and `INTERACTION-CONTRACT.md` is authoritative for accepted v0.1 interaction behavior. Do not resolve a conflict by inventing new semantics.

Do not silently change the format or accepted design decisions merely to simplify an implementation. If implementation experience exposes a flaw in the specification or an accepted design decision, stop and report the conflict explicitly.

## Current implementation architecture

The reference implementation is written in Rust and is migrating toward explicit reusable layers:

- `carta-format`: format types, parsing, serialization, validation, and compatibility rules.
- `carta-core`: canonical Archive/domain operations: Documents, Volumes, Works, LEAP retrieval, Git history, Trash/Wipe, synchronization and backlinks.
- `carta-app`: frontend-independent interactive application primitives. It currently owns the editor/Cat highlight/undo model, session model, View model, scheduler, palette matching and the first semantic `Action` values; more application semantics move here only when they are genuinely frontend-independent.
- `carta-publish`: frontend-independent publication.
- `carta-cli`: Unix-style administrative interface.
- `carta-tui`: frozen terminal frontend from the v0.1.x line. It remains a regression harness and compatibility frontend, not the place for new v0.2 features.
- `carta-gui`: planned canonical v0.2 desktop frontend, to be introduced only after the physical-keyboard input probe succeeds.

Archive semantics belong in `carta-core`. Reusable interaction semantics belong in `carta-app`. Platform input, rendering, clipboard, windowing and OS integration belong in frontend adapters.

For the frozen TUI, Crossterm-specific keyboard interpretation belongs in `crates/carta-tui/src/input.rs`, not in `carta-app`. Pending/active physical LEAP keys, suppressed key-release quirks, Right Control tracking and terminal keycode mapping are adapter state.
The internal fields of that adapter state are private. Other TUI modules should use its transition methods rather than manipulating pending/active LEAP or modifier bookkeeping directly.

No frontend may grow a second implementation of the editor, Cat selection, LEAP semantics, Views or other shared application behavior merely for convenience. Future desktop, web and mobile frontends should reuse `carta-app` and `carta-core` to the maximum practical extent.

## Interaction

Carta Space defines two momentary LEAP controls:

- LEAP backward
- LEAP forward

The final v0.1 TUI maps them to physical Left Control for LEAP backward and physical Left Alt for LEAP forward when terminal event fidelity is sufficient. The v0.2 desktop frontend must preserve the real momentary LEAP gesture using physical press/release events and must keep Right Alt/AltGr available for normal international text entry.

Frontends translate physical input into semantic Carta actions. Do not encode platform key codes or toolkit events into the archive format, `carta-core`, or reusable editor state.

## Interaction philosophy

Carta Space is writing-first. The default interactive experience should optimize for producing and continuing text, not for managing the software.

When designing frontend behavior:

- do not require filenames, directories, tags, templates, properties, or other classification before a user can begin writing;
- prefer a quiet text surface over permanently visible organizational chrome;
- make secondary capabilities available on demand and let them disappear when the operation is complete;
- prefer transient commands, Views, palettes, or quasimodes over persistent panels when they solve the same problem adequately;
- keep authorship separate from publication and final visual formatting;
- remember that a dedicated distraction-free writer, richer desktop frontend, and lightweight capture frontend may legitimately expose different subsets of the same core capabilities;
- do not reduce backend capability merely to preserve a superficial minimalist aesthetic if automation can remove work from the user.

The guiding interaction rule is:

> Power should be available on demand, not presented by default.

## Design rules

- Prefer simple, standard technology over custom mechanisms.
- Preserve the writing-first interaction model: organization should normally follow content rather than precede it.
- Do not add speculative features.
- Do not invent v0.1 interaction behavior that is already decided in `INTERACTION-CONTRACT.md`; report contradictions or missing cases instead.
- Treat Trash and Wipe as distinct operations with the semantics defined by the interaction contract and specification.
- Do not add tags, AI features, synchronization, collaboration, bibliography, or asset systems unless explicitly requested.
- Canonical authored data must remain recoverable from ordinary Markdown and JSON files.
- Treat Views as derived projections over canonical Archive state; Draft 0.1 has no persistent View object or `views/` directory.
- Never duplicate Document content to implement a View or Work.
- Internal Document/Work links, when implemented, use ordinary CommonMark links with the reserved `carta:doc:<id>` and `carta:work:<id>` destinations defined by the specification.
- Treat backlinks as derived relationships, never as a second canonical list that must be synchronized with outgoing links.
- Full-text, link/backlink, semantic/vector, and similar indexes must remain disposable derived state whose loss cannot destroy authored or structural Archive data.
- Do not introduce semantic indexing or LLM-dependent retrieval merely in anticipation of future scale; require a demonstrated retrieval problem.
- Document and Work identities are stable.
- Document boundaries in composite views are structural and non-editable.
- Preserve unknown future metadata wherever possible.
- Keep UI, storage format, and core behavior cleanly separated.
- Do not put terminal columns, GUI pixels, font metrics, Crossterm/Winit key types, or other presentation geometry into shared `Action` values. Visual navigation needs an explicit cross-frontend contract before extraction.
- Prefer small, explicit interfaces between crates.
- Avoid introducing hidden global state when a clear domain object or explicit dependency is possible.
- Use one deterministic Carta XDG data root: `$XDG_DATA_HOME/carta`, falling back to `~/.local/share/carta`. The default Archive is `archive/` below that root; device-local session sidecars live below the same root but outside the Archive Git working tree. An explicit startup/archive path may override the default; do not reintroduce a last-used Archive pointer.

The working rule is:

> Complexity must be earned by a demonstrated problem.

## Development environment on NixOS

The development host is NixOS.

Do **not** install or configure development dependencies directly into the NixOS host unless explicitly asked to do so.

Use `distrobox` as the default development environment mechanism.

Create and use a dedicated Debian container for Carta Space development. The preferred default is:

```bash
distrobox create --name carta-dev --image debian:stable
distrobox enter carta-dev
```

If the box already exists, reuse it.

Inside `carta-dev`, install and configure whatever development tools are needed for the task. This may include, as appropriate:

- Rust toolchain and Cargo;
- build-essential and pkg-config;
- Git;
- curl, wget, jq, unzip, zip;
- clang, lldb, gdb;
- cmake and ninja-build;
- libraries required by Rust crates;
- terminal/testing utilities;
- code quality tools such as rustfmt and clippy;
- other temporary build or diagnostic dependencies.

Prefer installing development tooling inside the Debian box rather than modifying the NixOS host.

The repository itself may remain in the user's normal home directory and be accessed from inside Distrobox through its normal home-directory integration.

Do not create additional containers unless there is a concrete need.

Do not delete or rebuild the development box merely to solve a local dependency problem; first try to repair or extend the existing environment.

When adding a new required system dependency, document it in the repository if it becomes part of the reproducible development setup.

## Development practice

Prefer small, testable changes.

Core and shared application behavior should be testable without launching any frontend.

When implementing a feature:

1. identify which specification or design decision governs it;
2. place canonical Archive/domain behavior in `carta-core`;
3. place reusable interactive behavior in `carta-app` rather than a frontend when practical;
4. add automated tests at the lowest appropriate layer;
5. keep frontend adapters thin: translate native input, execute platform effects, and render shared state;
6. run the relevant Rust test and lint suite;
7. report any case where implementation experience suggests the specification itself may be wrong rather than silently changing it.

For Rust code, prefer conventional tooling and idiomatic project structure.

Before considering a task complete, run the relevant subset of:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

If one of these commands cannot run because the project is still too early or a dependency is unavailable, explain why rather than pretending validation succeeded.

## Git discipline

Keep commits focused and descriptive.

Do not rewrite published history, force-push, or make destructive Git changes unless explicitly requested.

Do not commit generated build artifacts, editor caches, temporary files, or Distrobox/container state.

## Scope discipline

Do not broaden the task beyond what was requested.

If a small implementation decision is reversible and does not affect the format or architecture, choose the simplest reasonable option and proceed.

If a decision changes the format, archive semantics, LEAP interaction model, Trash/Wipe or history guarantees, or crate boundaries, surface it explicitly before making the change.


## Development workflow roles

The default workflow is defined in `DEVELOPMENT-WORKFLOW.md`.

For the current v0.2 development cycle, design/review and repository implementation are normally handled in ChatGPT. ChatGPT may inspect the repository, edit tracked source and documentation, create focused commits, and push them to the active development branch.

OpenCode is normally used as the local execution and runtime-verification agent. Unless a task explicitly says otherwise, it should not modify repository files. Its main responsibilities are to run builds, formatters, linters, automated tests, reproduce runtime failures, exercise the affected frontend, inspect terminal or GUI keyboard behavior, and report exact results.

The normal loop is therefore:

```text
user/dogfooding -> ChatGPT implementation -> local pull -> OpenCode verification -> ChatGPT review/fix
```

For current v0.2 work, the active development branch is `opencode/v0.2`. The final terminal snapshot is `release/v0.1.2-final`; do not develop new features there. Do not write implementation commits to `main` unless explicitly requested.

If local verification requires a code change, OpenCode should report the failure rather than silently patching it unless the task explicitly authorizes edits.


## Public repository privacy rule

The repository is public even though it is currently used as a private development workspace. Treat every push as publication on the Internet.

Before pushing, inspect the changed content for secrets and personal/local data. Do not commit real archive contents, private emails, credentials, tokens, SSH/WireGuard material, personal addresses, private IPs, unnecessary hostnames, absolute home paths, local logs, screenshots or OpenCode reports containing such data. Use synthetic fixtures and neutral examples.

If sensitive material is discovered after a push, do not assume a later deletion commit is sufficient: report it immediately because history rewriting and secret rotation may be required.
