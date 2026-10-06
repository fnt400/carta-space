#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
[[ $# == 2 ]] || { printf 'Usage: render.sh INPUT.typ OUTPUT.pdf\n' >&2; exit 2; }
here=$(dirname -- "$(realpath -- "$0")")
exec /opt/carta-pdf-bench/typst-0.15.1/typst compile \
  --root "$here" --font-path "$here/../fonts" \
  --ignore-system-fonts --ignore-embedded-fonts \
  --creation-timestamp 1791244800 "$1" "$2"
