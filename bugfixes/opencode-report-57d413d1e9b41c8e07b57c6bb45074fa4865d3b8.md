# Local verification report

Tested HEAD: `57d413d1e9b41c8e07b57c6bb45074fa4865d3b8`

## Repository state

- `git status --short` initially showed only `Cargo.lock` modified. Its diff contained only a dependency-order change in the `carta-tui` lockfile entry. The generated change was restored.
- `git pull --ff-only origin opencode/v0.2`: passed (fast-forward).
- `git rev-parse HEAD`: matched the expected HEAD.
- Working tree was clean after synchronization.

## Results

- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: passed.
- `cargo test --workspace`: passed; one Typst-dependent test was ignored by its test configuration.
- `cargo build --workspace`: passed.
- `cargo clippy --manifest-path crates/carta-gui/Cargo.toml --all-targets --all-features -- -D warnings`: passed.
- `cargo test --manifest-path crates/carta-gui/Cargo.toml`: passed (13 tests).
- `cargo build --manifest-path crates/carta-gui/Cargo.toml`: passed.
- `cargo fmt --check`: passed.
- `cargo fmt --manifest-path crates/carta-gui/Cargo.toml --check`: **failed**.

The GUI formatter reports formatting differences in `crates/carta-gui/src/input.rs`, `main.rs`, `ui.rs`, and `viewport.rs`. The command was check-only; no source files were changed.
