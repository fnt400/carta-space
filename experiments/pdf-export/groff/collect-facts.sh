#!/usr/bin/env bash
set -euo pipefail
[[ ${CONTAINER_ID:-} == carta-dev ]] || { printf 'Run inside carta-dev.\n' >&2; exit 1; }
here=$(cd -- "$(dirname -- "$0")" && pwd)
prefix=/opt/carta-pdf-bench/groff-1.24.1
export PATH="$prefix/bin:/usr/bin:/bin"
mkdir -p "$here/logs"
{
    date -u
    sha256sum /opt/carta-pdf-bench/groff-1.24.1.tar.gz
    groff --version
    gropdf --version
    printf '\nInstalled payload (source excluded):\n'
    du -sb "$prefix/bin" "$prefix/lib" "$prefix/share"
    printf '\nPrefix including initially co-located sources:\n'
    du -sb "$prefix"
    printf '\nBuild tree:\n'
    du -sb /opt/carta-pdf-bench/groff-build-1.24.1
    printf '\nFont artifacts:\n'
    du -sb "$prefix/share/groff/site-font"
    find "$prefix/share/groff/site-font" -type f -printf '%s %p\n'
    printf '\nFont symlinks:\n'
    find "$prefix/share/groff/site-font" -type l -printf '%p -> %l\n'
    printf '\nScratch retained by upstream installer:\n'
    find /opt/carta-pdf-bench/install-font -printf '%y %s %p\n'
    printf '\nRuntime ELF dependencies:\n'
    for exe in groff preconv troff; do
        printf '\n%s\n' "$exe"
        ldd "$prefix/bin/$exe"
    done
    printf '\ngropdf interpreter and imported Perl modules:\n'
    head -n 1 "$prefix/bin/gropdf"
    grep -E '^(use|require) ' "$prefix/bin/gropdf"
    /usr/bin/perl -MGetopt::Long -MExporter -MFile::Spec -MCompress::Zlib -MEncode -e 'print "Perl $^V; Compress::Zlib $Compress::Zlib::VERSION\n"'
    ldd /usr/bin/perl
    printf '\nPerl compression shared-library dependencies:\n'
    find /usr/lib -path '*/auto/Compress/Raw/*/*.so' -exec ldd '{}' \;
    printf '\nRelevant Debian package versions:\n'
    dpkg-query -W build-essential fontforge bison flex texinfo libuchardet-dev perl libcompress-raw-zlib-perl libcompress-raw-bzip2-perl libio-compress-perl poppler-utils
    printf '\nPDF facts:\n'
    for pdf in "$here"/*.pdf; do
        pdfinfo "$pdf"
        pdffonts "$pdf"
    done
} > "$here/logs/facts.log" 2>&1
