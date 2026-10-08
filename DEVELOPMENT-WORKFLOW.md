# Carta Space — Development Workflow

This document defines the default development workflow for the current Carta Space v0.2 cycle.

The goal is to keep product reasoning and source changes coherent while using the local coding agent only where direct access to the real development environment materially helps.

## Roles

### User / dogfooding

The user exercises Carta Space as a real writing environment and reports:

- bugs;
- interaction friction;
- desired behavior changes;
- terminal, GUI and keyboard observations;
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

### OpenCode — local verification and small mechanical corrections

OpenCode is used to verify ChatGPT's published commit in an appropriate
local Rust environment. Prefer **Luna Low** for routine checks. GitHub Actions
also run automated quality gates, but a passing CI result does not establish
GUI responsiveness.

The current v0.2 local environment is **`carta-gui-dev` (Ubuntu 26.04,
Rust >= 1.88)**, not the former Debian `carta-dev`. OpenCode may run
inside that Distrobox or invoke it when available. In an isolated agent
execution environment that lacks Distrobox, an equivalent complete native
Rust toolchain is acceptable for code checks; clearly identify that environment
and never imply it is the user's NixOS/Distrobox. If a valid execution
environment cannot be reached, report **BLOCKED** rather than **FAIL**.

Default verification: `bash scripts/verify-carta.sh`, including the
separate GUI debug and release builds. OpenCode may automatically fix
formatting, trivial Clippy lints, imports and syntax errors with an unambiguous
local fix, up to three small attempts, followed by complete re-verification.

OpenCode must not autonomously change program behavior or tests: LEAP, input,
rendering, scrolling, persistence, synchronization, architecture and
dependencies return to ChatGPT. It must not alter frozen TUI behavior.
Publish focused mechanical commits only after reviewing the diff for private
data. For a genuine unresolved test failure, push a sanitized
`bugfixes/opencode-report-<HEAD_TESTATO>.md` and return **FAIL**.

An environmental inability to execute is reported as **BLOCKED**, with the
specific unavailable tool/container and the commands not run. Do not
commit an environment-only failure report unless explicitly requested.

## Default loop

```text
user/dogfooding
    ↓
ChatGPT: analyse + implement + tests + commit + push
    ↓
GitHub Actions: fmt + Clippy + tests + builds + portability
    ↓
only when needed: local/OpenCode runtime or hardware verification
    ↓
ChatGPT: review evidence and fix if required
```

Repeat until the change is accepted by dogfooding.

## Active branch

For the current v0.2 development cycle the active branch is:

`opencode/v0.2`

Implementation work must not be committed to `main` unless explicitly requested.

## Scope rule

Each implementation commit should represent one coherent behavior change or one tightly related dogfooding pass.

Do not combine unrelated refactoring with functional changes.

When a requested change affects the archive format, canonical metadata, identity semantics, Trash/Wipe guarantees, history semantics, LEAP behavior, or crate boundaries, record the accepted decision in the appropriate project document.

## Local verification contract

A normal OpenCode verification prompt should:

1. state the repository, branch `opencode/v0.2`, and **exact expected HEAD**;
2. inspect the working tree, then `git pull --ff-only origin opencode/v0.2`;
3. identify the **actual execution environment** and Rust/toolchain version;
4. run `bash scripts/verify-carta.sh` from an adequate environment;
5. authorize only the small mechanical fixes described above;
6. distinguish **PASS** (all checks run), **FAIL** (tests/lints actually failed)
   and **BLOCKED** (environment prevents running them);
7. return the tested HEAD, any new commit, and a sanitized failure report for
   genuine code failures.

The Carta crates live inside this workspace; verification should not assume separate sibling repositories for them.

## Rust verification baseline

The automatic quality gate uses:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
```

Local workspace and standalone GUI checks are additionally available
through `bash scripts/verify-carta.sh`. Run this in the actual local
`carta-gui-dev` when possible, since remote agent sandboxes may not have the
same container runtime or libraries. Passing automated tests does not replace
real runtime verification for physical keyboard input, GUI rendering,
sustained responsiveness, export, recovery or other environment-dependent
behavior.

## Git discipline

ChatGPT creates focused commits on the development branch and pushes them.

The local checkout updates with:

```bash
git pull --ff-only origin opencode/v0.2
```

Do not force-push, rewrite published history, or commit local temporary files.

## Completion

A change is not considered accepted merely because it was committed.

For behavior visible in any frontend, acceptance requires the relevant local verification and, when appropriate, user dogfooding.


## v0.2 architecture discipline

The v0.1.2 terminal frontend is frozen. New v0.2 features must not be implemented only in `carta-tui`.

Shared interaction behavior belongs in `carta-app` when it is independent of a specific presentation technology. Canonical Archive behavior remains in `carta-core`. Frontends should be adapters around those layers.

In the TUI, Crossterm-specific input state and key mapping belong in `crates/carta-tui/src/input.rs`. Keep physical-key quirks there rather than promoting them into shared application semantics.
Keep the adapter state encapsulated: callers should use its API rather than reaching into pending/active modifier fields. This makes terminal-specific refactors independently testable.
When splitting input logic, keep a distinction between TUI-local interpreted intents and shared `carta-app::Action` values. Do not promote terminal gestures into shared actions merely to reduce `main.rs`.

The migration is incremental: do not move code merely to make directory diagrams look clean. Extract a behavior when its ownership is clear and preserve the frozen TUI as a regression oracle while the shared layer is being built.

The intended next seam is:

```text
native input -> semantic Action -> carta-app -> state / Effect -> platform adapter
```

Prompt, confirmation, and selector editing/navigation are already shared through `ModeAction`. The TUI-specific palette extension for Portable Keyboard Mode remains in `carta-tui`.

Do not design a large generic platform framework in advance. Introduce effects only for concrete host services such as clipboard, file selection/save, window integration or other operations that genuinely differ across desktop/web/mobile.

## Privacy before push

The GitHub repository is public. Before each push, review the diff as if it were being published immediately.

Use synthetic test data. Do not commit private archive text, credentials, personal contact information, private infrastructure details, unnecessary hostnames/IPs, absolute personal paths, logs or screenshots containing local data. OpenCode failure reports must be sanitized before commit.


## Automatic quality gate

The `Portability` workflow is also the automatic code-quality gate for active development branches.

Every push to `opencode/v0.2` must run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

in addition to the existing multi-platform workspace tests and release builds.

Do not ask the user to relay routine formatter/Clippy failures between ChatGPT and OpenCode when GitHub Actions can expose them directly. Inspect and fix CI failures before requesting another local verification pass.


## Pre-GUI preparation stop point

The structural extraction phase is complete when:

- the canonical `App`, editor, View/AppMode state, LEAP semantics, prompt/confirmation/selector semantics, help content, and reusable actions live in `carta-app`;
- Crossterm modifier state, Portable Keyboard Mode, terminal clipboard integration, and terminal rendering remain in `carta-tui`;
- `carta-app` has no frontend/toolkit dependency;
- the automatic quality and portability workflow is green.

Once these conditions are satisfied, do not continue speculative refactoring. The next work item is the desktop keyboard/input probe and requires an explicit toolkit/input-layer decision with the user.
