#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
here=$(cd -- "$(dirname -- "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
prefix=/opt/carta-pdf-bench/groff-1.24.1
stage=/opt/carta-pdf-bench/groff-pdf-runtime-measured
results="$here/../results"
cd "$repo"
mkdir -p "$here/logs" "$stage"
traces=$(mktemp -d "$here/logs/packaging-traces.XXXXXX")
cat "$here/unicode-fallback.roff" "$here/unicode.roff" > /opt/carta-pdf-bench/unicode-fallback-combined.roff
export PATH="$prefix/bin:/usr/bin:/bin"
unset GROFF_BIN_PATH GROFF_FONT_PATH GROFF_TMAC_PATH
for input in latin unicode-fallback; do
    inputfile="$here/latin.roff"
    [[ $input != unicode-fallback ]] || inputfile=/opt/carta-pdf-bench/unicode-fallback-combined.roff
    strace -ff -e trace=openat,execve -o "$traces/packaging-$input.trace" \
        "$prefix/bin/groff" -Kutf8 -Tpdf -P-pa4 -M "$here" -mcarta -ww "$inputfile" \
        > "/opt/carta-pdf-bench/packaging-$input.pdf" \
        2> "$here/logs/packaging-$input.stderr.log"
done
perl "$here/packaging-manifest.pl" "$prefix" "$here" "$traces" > "$results/groff-packaging.txt"
# The manifest is an observed, unstripped text-only closure, not an installation.
while IFS=$'\t' read -r tag bytes source; do
    [[ $tag == CORE || $tag == CUSTOM ]] || continue
    relative=${source#"$prefix/"}
    mkdir -p "$stage/$(dirname "$relative")"
    cp -L "$source" "$stage/$relative"
done < "$results/groff-packaging.txt"
mkdir -p "$stage/tmac"
cp "$here/carta.tmac" "$stage/tmac/"
for input in latin unicode-fallback; do
    inputfile="$here/latin.roff"
    [[ $input != unicode-fallback ]] || inputfile=/opt/carta-pdf-bench/unicode-fallback-combined.roff
    env GROFF_BIN_PATH="$stage/bin" \
        GROFF_FONT_PATH="$stage/share/groff/site-font:$stage/share/groff/1.24.1/font" \
        GROFF_TMAC_PATH="$stage/tmac:$stage/share/groff/1.24.1/tmac" \
        strace -ff -e trace=openat,execve -o "$traces/packaging-staged-$input.trace" \
        "$stage/bin/groff" -Kutf8 -Tpdf -P-pa4 -mcarta -ww "$inputfile" \
        > "/opt/carta-pdf-bench/packaging-staged-$input.pdf" \
        2> "$here/logs/packaging-staged-$input.stderr.log"
    cmp "/opt/carta-pdf-bench/packaging-$input.pdf" "/opt/carta-pdf-bench/packaging-staged-$input.pdf" \
        > "$here/logs/packaging-staged-$input.cmp.log" || true
done
{
    printf '\nCOPIED-TREE VALIDATION (no installed-tree modifications):\n'
    for input in latin unicode-fallback; do
        pdftotext -raw "/opt/carta-pdf-bench/packaging-$input.pdf" "/opt/carta-pdf-bench/packaging-$input.txt"
        pdftotext -raw "/opt/carta-pdf-bench/packaging-staged-$input.pdf" "/opt/carta-pdf-bench/packaging-staged-$input.txt"
        cmp "/opt/carta-pdf-bench/packaging-$input.txt" "/opt/carta-pdf-bench/packaging-staged-$input.txt"
        printf '%s: extracted text identical; ' "$input"
        qpdf --check "/opt/carta-pdf-bench/packaging-staged-$input.pdf"
    done
    printf '\nCompiled-prefix successful opens in copied-tree test:\n'
    perl -ne 'print if /openat\([^,]+, "\/opt\/carta-pdf-bench\/groff-1\.24\.1\/.*\)\s*=\s*\d+/' "$traces"/packaging-staged-*.trace.*
    printf '\nCore dependency ldd (outside byte totals):\n'
    for exe in "$prefix/bin/groff" "$prefix/bin/preconv" "$prefix/bin/troff" /usr/bin/perl; do
        printf '%s\n' "$exe"
        ldd "$exe"
    done
    printf '\nPACKAGE SCOPE: 72 unstripped files, no directory overhead, not an absolute lower bound.\n'
    printf 'gropdf retains compiled search roots: staged rendering still reads original download registries.\n'
    printf 'For relocation, regenerate compiled paths or rebuild with the deployment prefix.\n'
    printf 'Perl observed module sizes are workload-specific; full Perl and ELF libraries are separate.\n'
} >> "$results/groff-packaging.txt" 2>&1
