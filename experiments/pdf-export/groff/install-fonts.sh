#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
here=$(cd -- "$(dirname -- "$0")" && pwd)
prefix=/opt/carta-pdf-bench/groff-1.24.1
export PATH="$prefix/bin:/usr/bin:/bin"
mkdir -p "$here/logs" /opt/carta-pdf-bench/install-font
upstream=$(<"$prefix/share/doc/groff-1.24.1/examples/install-font.bash")
# Relocate upstream scratch files only; conversion code remains unchanged.
upstream=${upstream//tmp_dir=\/tmp\/install-font/tmp_dir=\/opt\/carta-pdf-bench\/install-font}
cd "$here/../fonts"
for pair in \
    SourceSerif4-Regular.ttf:CR SourceSerif4-Italic.ttf:CI \
    SourceSerif4-Semibold.ttf:CB SourceCodePro-Regular.ttf:CM \
    NotoSans-Regular.ttf:NS NotoSansHebrew-Regular.ttf:NH \
    NotoSansArabic-Regular.ttf:NA NotoSansDevanagari-Regular.ttf:ND \
    NotoSansJP-Regular.ttf:NJ; do
    file=${pair%:*}
    name=${pair#*:}
    [[ -f $file ]] || { printf 'Missing %s\n' "$file" >&2; exit 1; }
    bash -c "$upstream" install-font -P "$prefix/share/groff" -n -d -F Carta -f "$name" "$file" > "$here/logs/font-$name.log" 2>&1
done
