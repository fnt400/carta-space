#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
export RUSTUP_HOME=/opt/carta-pdf-bench/rustup
export CARGO_HOME=/opt/carta-pdf-bench/cargo
export CARGO_TARGET_DIR=/opt/carta-pdf-bench/printpdf-target
export PATH="$CARGO_HOME/bin:$PATH"
here=$(dirname "$(realpath "$0")")
cargo +1.94.0 build --release --locked --manifest-path "$here/Cargo.toml"
cargo +1.94.0 fmt --check --manifest-path "$here/Cargo.toml"
cargo +1.94.0 clippy --locked --all-targets --all-features --manifest-path "$here/Cargo.toml" -- -D warnings
