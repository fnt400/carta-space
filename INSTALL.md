# Installing Carta Space

Carta Space v0.1.0 supports Linux. The interactive program is `carta`; the optional administrative CLI is `carta-cli`.

Git is a runtime dependency because Carta archives use Git for history and synchronization.

## Choose an installation method

| System | Recommended method |
| --- | --- |
| Recent glibc-based Linux, x86_64 | Prebuilt release binary |
| Debian, Ubuntu, Fedora, Arch, openSUSE, Alpine, Gentoo, Void, etc. | Build from source if the prebuilt binary is unsuitable |
| NixOS or any system with Nix flakes | Native Nix package |
| Linux on architectures other than x86_64 | Build from source or use Nix when supported |

## 1. Prebuilt Linux x86_64 release

The v0.1.0 release contains `carta` and `carta-cli`.

```bash
wget https://github.com/fnt400/carta-space/releases/download/v0.1.0/carta-v0.1.0-linux-x86_64.tar.gz
wget https://github.com/fnt400/carta-space/releases/download/v0.1.0/carta-v0.1.0-linux-x86_64.tar.gz.sha256

sha256sum -c carta-v0.1.0-linux-x86_64.tar.gz.sha256
tar -xzf carta-v0.1.0-linux-x86_64.tar.gz
cd carta-v0.1.0-linux-x86_64

install -Dm755 carta ~/.local/bin/carta
install -Dm755 carta-cli ~/.local/bin/carta-cli   # optional
```

Make sure `~/.local/bin` is in your `PATH`, then run:

```bash
carta
```

The release binary is dynamically linked and was built on a current Linux userspace. It is intended for recent glibc-based x86_64 distributions. NixOS does not run generic dynamically linked Linux binaries directly; use the Nix method below. On older glibc systems, musl-based systems such as Alpine, or other architectures, build from source.

## 2. Build from source

This is the broadest installation method.

### Build requirements

- Rust 1.85 or newer and Cargo;
- Git;
- a C compiler/build toolchain;
- pkg-config or pkgconf;
- Wayland development files.

If your distribution's Rust is older than 1.85, install a current toolchain with [rustup](https://rustup.rs/).

Common build dependencies:

### Debian / Ubuntu

```bash
sudo apt update
sudo apt install git build-essential pkg-config libwayland-dev
```

### Fedora

```bash
sudo dnf install git gcc pkgconf-pkg-config wayland-devel
```

### Arch Linux / Manjaro

```bash
sudo pacman -S git base-devel wayland
```

### Alpine Linux

```bash
sudo apk add git build-base pkgconf wayland-dev
```

For other distributions, install the equivalent packages: Git, a C build toolchain, pkg-config/pkgconf, and Wayland development headers/libraries.

Then install Carta:

```bash
git clone --depth 1 --branch v0.1.0 https://github.com/fnt400/carta-space.git
cd carta-space

cargo install --locked --path crates/carta-tui
cargo install --locked --path crates/carta-cli   # optional
```

Cargo normally installs binaries under `~/.cargo/bin`. Ensure that directory is in your `PATH`.

Check the installation:

```bash
carta --version
carta-cli --version
```

## 3. Nix / NixOS

Carta includes a native flake.

Run without installing:

```bash
nix run github:fnt400/carta-space
```

Install into your current Nix profile:

```bash
nix profile install github:fnt400/carta-space
```

Then:

```bash
carta
```

The Nix package includes Git in Carta's runtime environment and does not require `steam-run` or `nix-ld`.

## First launch

With no existing default Archive, Carta asks whether to:

- create a new empty Archive;
- import an existing Git-backed Carta Archive;
- quit.

The default Archive is stored at `$XDG_DATA_HOME/carta/archive`, falling back to `~/.local/share/carta/archive`.

## Updating

For a manually installed prebuilt release, replace the two binaries with those from a newer release.

For a source installation, check out the desired release and repeat `cargo install --locked --path ...`.

For Nix:

```bash
nix profile upgrade --all
```
