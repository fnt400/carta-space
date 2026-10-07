# Installing Carta Space

Carta Space v0.1.2 is the final terminal-interface release. It supports Linux and macOS. The interactive program is `carta`; the optional administrative CLI is `carta-cli`. Development from v0.2 moves to the graphical frontend.

Git is a runtime dependency because Carta archives use Git for history and synchronization.

Built-in PDF publication requires Typst 0.15.1 or newer on `PATH`. This is an optional runtime dependency: Carta and Markdown export continue to work without it. The Nix package supplies Typst automatically.

## Choose an installation method

| System | Recommended method |
| --- | --- |
| Linux x86_64 with glibc 2.31+ | Prebuilt Linux release binary |
| Debian, Ubuntu, Fedora, Arch, openSUSE, Alpine, Gentoo, Void, etc. | Build from source if the prebuilt binary is unsuitable |
| macOS 11+ on Apple Silicon | Prebuilt macOS arm64 binary |
| macOS 11+ on Intel | Prebuilt macOS x86_64 binary |
| NixOS or any system with Nix flakes | Native Nix package |
| Linux on architectures other than x86_64 | Build from source or use Nix when supported |

## 1. Prebuilt Linux x86_64 release

The v0.1.2 release contains `carta` and `carta-cli`.

```bash
wget https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-linux-x86_64.tar.gz
wget https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-linux-x86_64.tar.gz.sha256

sha256sum -c carta-v0.1.2-linux-x86_64.tar.gz.sha256
tar -xzf carta-v0.1.2-linux-x86_64.tar.gz
cd carta-v0.1.2-linux-x86_64

install -Dm755 carta ~/.local/bin/carta
install -Dm755 carta-cli ~/.local/bin/carta-cli   # optional
```

Make sure `~/.local/bin` is in your `PATH`, then run:

```bash
carta
```

The v0.1.2 Linux binary is dynamically linked and built/tested on Ubuntu 20.04 (glibc 2.31, Git 2.25.1). NixOS does not run generic dynamically linked Linux binaries directly; use the Nix method below. On systems with older glibc, musl-based systems such as Alpine, or other architectures, build from source.

## 2. Prebuilt macOS binaries

Download the package matching your CPU.

Apple Silicon:

```bash
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-arm64.tar.gz
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-arm64.tar.gz.sha256
shasum -a 256 -c carta-v0.1.2-macos-arm64.tar.gz.sha256
tar -xzf carta-v0.1.2-macos-arm64.tar.gz
cd carta-v0.1.2-macos-arm64
```

Intel:

```bash
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-x86_64.tar.gz
curl -LO https://github.com/fnt400/carta-space/releases/download/v0.1.2/carta-v0.1.2-macos-x86_64.tar.gz.sha256
shasum -a 256 -c carta-v0.1.2-macos-x86_64.tar.gz.sha256
tar -xzf carta-v0.1.2-macos-x86_64.tar.gz
cd carta-v0.1.2-macos-x86_64
```

Then install either build with:

```bash
mkdir -p ~/.local/bin
install -m 755 carta ~/.local/bin/carta
install -m 755 carta-cli ~/.local/bin/carta-cli   # optional
```

Carta's macOS binaries use a macOS 11 deployment target and are CI-tested on Apple Silicon and Intel. They are currently not Apple Developer signed or notarized. If Gatekeeper blocks a quarantined binary, build Carta from source instead.

## 3. Build from source

This is the broadest installation method.

### Linux build requirements

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

For other Linux distributions, install the equivalent packages: Git, a C build toolchain, pkg-config/pkgconf, and Wayland development headers/libraries.

The source build is also continuously tested on Alpine Linux/musl.

### macOS build requirements

Install the Xcode Command Line Tools if they are not already present:

```bash
xcode-select --install
```

Install Rust 1.85 or newer with rustup, and make sure `git --version` works.

Then install Carta on either Linux or macOS:

```bash
git clone --depth 1 --branch v0.1.2 https://github.com/fnt400/carta-space.git
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

If you want built-in PDF export, also verify:

```bash
typst --version
```

Use Typst 0.15.1 or newer. The official Typst release binaries are self-contained and may be installed independently of the Rust toolchain.

## 4. Nix / NixOS

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

The Nix package includes Git and Typst in Carta's runtime environment and does not require `steam-run` or `nix-ld`.

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
