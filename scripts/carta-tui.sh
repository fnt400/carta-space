#!/usr/bin/env bash
set -euo pipefail

# Avvia Carta TUI dentro la distrobox di sviluppo.
# Gli eventuali argomenti passati allo script vengono inoltrati a carta-tui.
# Il fuso orario viene ereditato dal sistema host a ogni avvio.

host_timezone() {
    if [[ -n "${TZ:-}" ]]; then
        printf '%s\n' "$TZ"
        return
    fi

    if command -v timedatectl >/dev/null 2>&1; then
        local zone
        zone="$(timedatectl show -p Timezone --value 2>/dev/null || true)"
        if [[ -n "$zone" ]]; then
            printf '%s\n' "$zone"
            return
        fi
    fi

    local target
    target="$(readlink -f /etc/localtime 2>/dev/null || true)"
    if [[ "$target" == *"/zoneinfo/"* ]]; then
        printf '%s\n' "${target#*/zoneinfo/}"
    fi
}

HOST_TZ="$(host_timezone)"

if [[ -n "$HOST_TZ" ]]; then
    exec distrobox enter carta-dev -- env TZ="$HOST_TZ" bash -lc '
      cd "$HOME/software/git/carta-space"
      exec cargo run -p carta-tui -- "$@"
    ' bash "$@"
fi

exec distrobox enter carta-dev -- bash -lc '
  cd "$HOME/software/git/carta-space"
  exec cargo run -p carta-tui -- "$@"
' bash "$@"
