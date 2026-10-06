#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
if [ "$#" -ne 1 ]; then
    printf 'usage: bash benchmark.sh INPUT_DIRECTORY\n' >&2
    exit 1
fi
here=$(dirname "$(realpath "$0")")
renderer=/opt/carta-pdf-bench/printpdf-target/release/carta-printpdf-bench
failed=0
for name in latin unicode perf-1 perf-10 perf-100; do
    if [ ! -f "$1/$name.html" ]; then
        printf 'Missing input: %s/%s.html\n' "$1" "$name" >&2
        failed=1
        continue
    fi
    if timeout 180 /usr/bin/time -v "$renderer" "$1/$name.html" "$here/outputs/$name.pdf" > "$here/logs/$name-runtime.log" 2>&1; then
        pdfinfo "$here/outputs/$name.pdf" > "$here/logs/$name-info.log" 2>&1
        pdffonts "$here/outputs/$name.pdf" > "$here/logs/$name-fonts.log" 2>&1
        pdftotext -layout "$here/outputs/$name.pdf" "$here/outputs/$name.txt"
    else
        printf 'Rendering failed: %s (see logs/%s-runtime.log)\n' "$name" "$name" >&2
        failed=1
    fi
done
exit "$failed"
