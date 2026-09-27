# Carta Space — Development Workflow

This document defines the default development workflow for the current Carta Space v0.1 cycle.

The goal is to keep product reasoning and source changes coherent while using the local coding agent only where direct access to the real development environment materially helps.

## Roles

### User / dogfooding

The user exercises Carta Space as a real writing environment and reports:

- bugs;
- interaction friction;
- desired behavior changes;
- terminal and keyboard observations;
- failures that occur only in the real local environment.

Dogfooding evidence has priority over speculative UX work.

### ChatGPT — design, implementation, and review

ChatGPT is normally responsible for:

- architecture and interaction decisions;
- repository analysis;
- source-code and documentation changes;
- adding or updating automated tests in the repository;
- code review;
- debugging from source, logs, panic traces, and test output;
- creating focused commits;
- pushing those commits to the active development branch;
- preparing exact verification instructions for the local environment.

ChatGPT must keep changes scoped and must not silently change archive-format or accepted interaction semantics merely to simplify implementation.

### OpenCode — local verification agent

OpenCode is normally responsible for operations that require the real local environment:

- `cargo fmt --check`;
- Clippy;
- Cargo tests;
- builds;
- reproducing runtime failures;
- exercising the TUI;
- keyboard-event probes;
- terminal compatibility checks;
- filesystem/Git/Distrobox-dependent checks;
- reporting exact logs, backtraces, commands, and outcomes.

Unless a task explicitly authorizes edits, OpenCode must not modify repository files. If verification exposes a defect, it reports the defect and stops; the fix returns to ChatGPT.

## Default loop

```text
user/dogfooding
    ↓
ChatGPT: analyse + implement + tests + commit + push
    ↓
local: git pull --ff-only origin opencode/v0.1
    ↓
OpenCode: verify without modifying files
    ↓
ChatGPT: review evidence and fix if required
```

Repeat until the change is accepted by dogfooding.

## Active branch

For the current v0.1 development cycle the active branch is:

`opencode/v0.1`

Implementation work must not be committed to `main` unless explicitly requested.

The two pre-existing local untracked files `session-ses_f357.md` and `temp.txt` are not project changes and must not be committed.

## Scope rule

Each implementation commit should represent one coherent behavior change or one tightly related dogfooding pass.

Do not combine unrelated refactoring with functional changes.

When a requested change affects the archive format, canonical metadata, identity semantics, Trash/Wipe guarantees, history semantics, LEAP behavior, or crate boundaries, record the accepted decision in the appropriate project document.

## Local verification contract

A normal OpenCode verification prompt should:

1. state the exact repository path: `/path/to/carta-space`;
2. state branch `opencode/v0.1`;
3. explicitly say **do not modify files**;
4. list the exact commands and runtime checks;
5. ask for raw failure output when something fails;
6. stop after reporting results.

OpenCode must not request access to sibling repositories such as `~/software/git/carta-core` or `~/software/git/carta-tui`; those are crates inside the Carta Space workspace, not separate repositories.

## Rust verification baseline

When applicable, local verification uses:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Passing automated tests does not replace real runtime verification for terminal, keyboard, rendering, export, recovery, or other environment-dependent behavior.

## Git discipline

ChatGPT creates focused commits on the development branch and pushes them.

The local checkout updates with:

```bash
git pull --ff-only origin opencode/v0.1
```

Do not force-push, rewrite published history, or commit local temporary files.

## Completion

A change is not considered accepted merely because it was committed.

For behavior visible in the TUI, acceptance requires the relevant local verification and, when appropriate, user dogfooding.
