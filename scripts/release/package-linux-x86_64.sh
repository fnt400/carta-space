#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

for command in cargo tar sha256sum install awk; do
    command -v "$command" >/dev/null 2>&1 || {
        printf 'package-linux-x86_64: required command not found: %s\n' "$command" >&2
        exit 1
    }
done

version="$(
    awk '
        /^\[workspace\.package\]$/ { in_package = 1; next }
        /^\[/ { in_package = 0 }
        in_package && $1 == "version" && $2 == "=" {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' Cargo.toml
)"

if [[ -z "$version" ]]; then
    printf 'package-linux-x86_64: cannot determine workspace version\n' >&2
    exit 1
fi

target_dir="${CARGO_TARGET_DIR:-target}"
if [[ "$target_dir" != /* ]]; then
    target_dir="$repo_root/$target_dir"
fi

dist_dir="$repo_root/dist"
package_name="carta-v${version}-linux-x86_64"
stage_dir="$dist_dir/$package_name"
archive_path="$dist_dir/$package_name.tar.gz"
checksum_path="$archive_path.sha256"

printf 'Building Carta Space v%s release binaries...\n' "$version"
cargo build --release --workspace

for binary in carta carta-cli; do
    if [[ ! -x "$target_dir/release/$binary" ]]; then
        printf 'package-linux-x86_64: missing release binary: %s\n' "$target_dir/release/$binary" >&2
        exit 1
    fi
done

rm -rf "$stage_dir"
mkdir -p "$stage_dir/LICENSES"

install -m 0755 "$target_dir/release/carta" "$stage_dir/carta"
install -m 0755 "$target_dir/release/carta-cli" "$stage_dir/carta-cli"
install -m 0644 README.md "$stage_dir/README.md"
install -m 0644 LICENSE "$stage_dir/LICENSE"
install -m 0644 LICENSES.md "$stage_dir/LICENSES.md"
install -m 0644 LICENSES/CC0-1.0.txt "$stage_dir/LICENSES/CC0-1.0.txt"
install -m 0644 LICENSES/OFL-source-serif.txt "$stage_dir/LICENSES/OFL-source-serif.txt"
install -m 0644 LICENSES/OFL-source-code-pro.txt "$stage_dir/LICENSES/OFL-source-code-pro.txt"
install -m 0644 LICENSES/OFL-noto-fonts.txt "$stage_dir/LICENSES/OFL-noto-fonts.txt"
install -m 0644 LICENSES/OFL-noto-cjk.txt "$stage_dir/LICENSES/OFL-noto-cjk.txt"

rm -f "$archive_path" "$checksum_path"
tar -C "$dist_dir" -czf "$archive_path" "$package_name"
(
    cd "$dist_dir"
    sha256sum "$package_name.tar.gz" > "$package_name.tar.gz.sha256"
)

rm -rf "$stage_dir"

printf '\nCreated:\n  %s\n  %s\n' "$archive_path" "$checksum_path"
