#!/usr/bin/env bash
set -euo pipefail

# Avvia Carta TUI dentro la distrobox di sviluppo.
# Gli eventuali argomenti passati allo script vengono inoltrati a carta-tui.

exec distrobox enter carta-dev -- bash -lc '
  cd "$HOME/software/git/carta-space"
  exec cargo run -p carta-tui -- "$@"
' bash "$@"
