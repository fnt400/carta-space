#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run via distrobox enter carta-dev -- bash %s input.roff output.pdf\n' "$0" >&2; exit 1; }
[[ $# == 2 ]] || { printf 'Usage: %s input.roff output.pdf\n' "$0" >&2; exit 2; }
here=$(cd -- "$(dirname -- "$0")" && pwd)
prefix=/opt/carta-pdf-bench/groff-1.24.1
export PATH="$prefix/bin:/usr/bin:/bin"
unset GROFF_BIN_PATH GROFF_FONT_PATH GROFF_TMAC_PATH
"$prefix/bin/groff" -Kutf8 -Tpdf -P-pa4 -M "$here" -mcarta -ww "$1" > "$2" 2> "$2.stderr.log"
