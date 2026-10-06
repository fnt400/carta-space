#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
root=/opt/carta-pdf-bench
prefix=$root/groff-1.24.1
here=$(cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$here/logs"
curl -fL --connect-timeout 15 --max-time 120 https://mirrors.kernel.org/gnu/groff/groff-1.24.1.tar.gz -o "$root/groff-1.24.1.tar.gz" 2> "$here/logs/download.log"
printf '74e2819795b6aff431aeac983d63a9c8968eeaba2a2eba7df8ba4c7b41e7cfd8  %s\n' "$root/groff-1.24.1.tar.gz" | sha256sum -c - > "$here/logs/checksum.log"
# Match the measured build: sources and installed payload share the prefix.
mkdir -p "$root/groff-build-1.24.1"
tar -xzf "$root/groff-1.24.1.tar.gz" -C "$root"
cd "$root/groff-build-1.24.1"
"$prefix/configure" --prefix="$prefix" > "$here/logs/configure.log" 2>&1
make -j2 > "$here/logs/build.log" 2>&1
make install > "$here/logs/install.log" 2>&1
