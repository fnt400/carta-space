# Carta Space — Agent Instructions

Carta Space is a content-first document environment inspired by Jef Raskin's Canon Cat and humane-interface work.

Before making architectural or implementation changes, read:

1. `SPECIFICATION.md` — normative definition of the Carta Space format.
2. `DESIGN-DECISIONS.md` — accepted architectural decisions and rationale.
3. `WHITEPAPER.md` — project philosophy and conceptual model.
4. `README.md` — current project overview.

If these documents appear to conflict, `SPECIFICATION.md` is authoritative for the archive format.

Do not silently change the format or accepted design decisions merely to simplify an implementation. If implementation experience exposes a flaw in the specification or an accepted design decision, stop and report the conflict explicitly.

## Current implementation architecture

The reference implementation is written in Rust and should remain divided into independent layers:

- `carta-format`: format types, parsing, serialization, validation, and compatibility rules.
- `carta-core`: Documents, Volumes, Works, LEAP, Git history, purge, import/export, and archive operations.
- `carta-cli`: Unix-style command-line interface.
- `carta-tui`: interactive terminal frontend using Ratatui and Crossterm.

Archive semantics belong in the core, not in the TUI.

The TUI is only one frontend. The design must permit future GTK, Emacs, web, or other frontends without reimplementing archive semantics.

## Interaction

Carta Space defines two momentary LEAP controls:

- LEAP backward
- LEAP forward

The reference TUI currently maps them experimentally to the physical left Control and right Control keys.

Do not encode those physical key bindings into the archive format or core domain model.

## Design rules

- Prefer simple, standard technology over custom mechanisms.
- Do not add speculative features.
- Do not add tags, AI features, synchronization, collaboration, bibliography, or asset systems unless explicitly requested.
- Canonical authored data must remain recoverable from ordinary Markdown and JSON files.
- Never duplicate Document content to implement a View or Work.
- Document and Work identities are stable.
- Document boundaries in composite views are structural and non-editable.
- Preserve unknown future metadata wherever possible.
- Keep UI, storage format, and core behavior cleanly separated.
- Prefer small, explicit interfaces between crates.
- Avoid introducing hidden global state when a clear domain object or explicit dependency is possible.

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

Core behavior should be testable without launching the TUI.

When implementing a feature:

1. identify which specification or design decision governs it;
2. implement or extend the core behavior first;
3. add automated tests;
4. expose it through CLI or TUI only afterwards;
5. run the relevant Rust test and lint suite;
6. report any case where implementation experience suggests the specification itself may be wrong rather than silently changing it.

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

If a decision changes the format, archive semantics, LEAP interaction model, deletion/history guarantees, or crate boundaries, surface it explicitly before making the change.
