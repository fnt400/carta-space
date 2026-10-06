#!/usr/bin/env bash
set -euo pipefail

# Avvia Carta TUI dentro la distrobox di sviluppo.
# Gli eventuali argomenti passati allo script vengono inoltrati a carta-tui.
# Locale e fuso orario vengono ereditati dal sistema host a ogni avvio.

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

ENV_ARGS=()
HOST_TZ="$(host_timezone)"
if [[ -n "$HOST_TZ" ]]; then
    ENV_ARGS+=("TZ=$HOST_TZ")
fi

for name in LANG LC_ALL LC_TIME LC_MESSAGES LC_CTYPE; do
    value="${!name:-}"
    if [[ -n "$value" ]]; then
        ENV_ARGS+=("$name=$value")
    fi
done

exec distrobox enter carta-dev -- env "${ENV_ARGS[@]}" bash -lc '
  # Development-only fallback: use the pinned Typst from the PDF benchmark
  # unless the caller already selected another renderer explicitly.
  if [[ -z "${CARTA_TYPST:-}" && -x /opt/carta-pdf-bench/typst-0.15.1/typst ]]; then
    export CARTA_TYPST=/opt/carta-pdf-bench/typst-0.15.1/typst
  fi

  cd "$HOME/software/git/carta-space"
  exec cargo run -p carta-tui -- "$@"
' bash "$@"
