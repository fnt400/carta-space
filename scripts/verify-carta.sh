#!/usr/bin/env bash
# Run the v0.2 Rust verification suite from an environment with Rust >= 1.88.
# Prefer the existing Ubuntu 26.04 "carta-gui-dev" Distrobox on NixOS.
# Never starts/stops containers, installs dependencies, or changes Git state.
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in rustc cargo git; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'BLOCKED: missing tool: %s\n' "$tool" >&2
        exit 2
    }
done

rust_version="$(rustc --version | awk '{print $2}')"
printf 'Rust:  %s\n' "$(rustc --version)"
printf 'Cargo: %s\n' "$(cargo --version)"
if ! printf '%s\n%s\n' 1.88 "$rust_version" | sort -V -C; then
    printf 'BLOCKED: GUI requires Rust >= 1.88 (found %s)\n' "$rust_version" >&2
    exit 2
fi

run() {
    printf '\n==> '
    printf '%q ' "$@"
    printf '\n'
    "$@"
}

run cargo fmt --check
run cargo fmt --manifest-path crates/carta-gui/Cargo.toml --check
run cargo clippy --workspace --all-targets --all-features -- -D warnings
run cargo test --workspace
run cargo build --workspace
run cargo clippy --manifest-path crates/carta-gui/Cargo.toml --all-targets --all-features -- -D warnings
run cargo test --manifest-path crates/carta-gui/Cargo.toml
run cargo build --manifest-path crates/carta-gui/Cargo.toml
run cargo build --release --manifest-path crates/carta-gui/Cargo.toml
run git diff --check

printf '\nCompleted all requested build/test checks.\n'
printf 'Review local changes before committing:\n'
git status --short
printf '\nPASS: compilation, tests, Clippy and formatting (NOT runtime performance).\n'
