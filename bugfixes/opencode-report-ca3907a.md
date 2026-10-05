# Verification report: FAIL

- Repository: `fnt400/carta-space`
- Branch: `opencode/v0.2`
- Tested HEAD: `ca3907af7f8ad99af1d5d15bbe5343d57436f1c4`
- Expected HEAD: `ca3907af7f8ad99af1d5d15bbe5343d57436f1c4` (PASS)
- Environment: existing Debian `carta-dev` Distrobox on NixOS.
- Toolchain: `rustc 1.85.1 (4eb161250 2025-03-15) (built from a source tarball)`; `cargo 1.85.1 (d73d2caf9 2024-12-31)`.
- Initial and post-validation tracked working tree: clean.

## Commands and results

Startup commands, executed in the repository root:

```sh
git switch opencode/v0.2
git pull --ff-only origin opencode/v0.2
git rev-parse HEAD
```

All succeeded. Pull fast-forwarded to the expected HEAD.

Environment and repository inspection commands:

```sh
git status --short
distrobox list
distrobox enter carta-dev -- bash -lc 'rustc --version && cargo --version'
git status --short
git diff
git log --oneline -10
```

All succeeded. The following validation commands were executed from the repository root, each prefixed with `distrobox enter carta-dev --`:

| Command | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | PASS |
| `cargo test --workspace` | PASS: 251 tests passed, none failed; doc-tests passed |
| `cargo build --workspace` | PASS |
| `cargo clippy --manifest-path experiments/carta-gui/Cargo.toml --all-targets -- -D warnings` | FAIL: incompatible dependency MSRVs |
| `cargo test --manifest-path experiments/carta-gui/Cargo.toml` | FAIL: incompatible dependency MSRVs |
| `cargo build --manifest-path experiments/carta-gui/Cargo.toml` | FAIL: incompatible dependency MSRVs |

Neither `cargo fmt --check` nor `cargo fmt --manifest-path experiments/carta-gui/Cargo.toml --check` was run: the required preceding validation did not pass. No formatter was run.

## GUI failure output

Clippy initially resolved a new, ignored GUI lockfile and reported:

```text
Updating crates.io index
Locking 381 packages to latest compatible versions
Adding eframe v0.32.3 (available: v0.36.2, requires Rust 1.95)
Adding winit v0.30.12 (available: v0.30.13)
```

All three GUI commands then failed with the same error, before source compilation:

```text
error: rustc 1.85.1 is not supported by the following packages:
  calloop@0.14.5 requires rustc 1.86.0
  darling@0.24.1 requires rustc 1.88.0
  darling_core@0.24.1 requires rustc 1.88.0
  darling_macro@0.24.1 requires rustc 1.88.0
  icu_collections@2.3.0 requires rustc 1.88
  icu_locale_core@2.3.0 requires rustc 1.88
  icu_normalizer@2.3.0 requires rustc 1.88
  icu_normalizer_data@2.3.0 requires rustc 1.88
  icu_normalizer_data@2.3.0 requires rustc 1.88
  icu_normalizer_data@2.3.0 requires rustc 1.88
  icu_properties@2.3.0 requires rustc 1.88
  icu_properties_data@2.3.0 requires rustc 1.88
  icu_properties_data@2.3.0 requires rustc 1.88
  icu_properties_data@2.3.0 requires rustc 1.88
  icu_provider@2.3.1 requires rustc 1.88
  idna_adapter@1.2.2 requires rustc 1.86
  image@0.25.10 requires rustc 1.88.0
  instability@0.3.14 requires rustc 1.88
  uuid@1.27.0 requires rustc 1.89.0
Either upgrade rustc or select compatible dependency versions with
`cargo update <name>@<current-ver> --precise <compatible-ver>`
where `<compatible-ver>` is the latest version supporting rustc 1.85.1
```

## Diagnosis and scope

The experimental GUI is a separate workspace with resolver `2`, declares Rust `1.85`, and ignores its `Cargo.lock`. Pinning eframe to `0.32.3` and winit to `0.30.12` does not constrain the complete dependency graph to Rust-1.85-compatible versions. Fresh dependency resolution in this environment selected packages requiring newer compilers.

No logic, dependency declarations, keybindings, architecture, or formatting were changed. Cargo generated the ignored `experiments/carta-gui/Cargo.lock`; it is not included in the report commit. No dependency downgrades or toolchain upgrades were attempted. GUI source correctness and physical LEAP handling remain unverified because compilation was blocked. Only this report is to be committed and pushed.
