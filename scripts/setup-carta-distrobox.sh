#!/usr/bin/env bash
set -euo pipefail

# Prepara la Distrobox di sviluppo di Carta Space.
# Carta non viene installata nel container: il repository resta nel normale
# $HOME condiviso da Distrobox e viene avviato con scripts/carta-tui.sh.
#
# Variabili opzionali:
#   CARTA_DISTROBOX_NAME   nome del container (default: carta-dev)
#   CARTA_DISTROBOX_IMAGE  immagine Distrobox (default: debian:stable)

BOX_NAME="${CARTA_DISTROBOX_NAME:-carta-dev}"
BOX_IMAGE="${CARTA_DISTROBOX_IMAGE:-debian:stable}"

fail() {
    printf 'setup-carta-distrobox: %s\n' "$*" >&2
    exit 1
}

command -v distrobox >/dev/null 2>&1 || fail "distrobox non trovato nel sistema host"

box_exists() {
    distrobox list --no-color 2>/dev/null |
        awk -F'|' -v wanted="$BOX_NAME" '
            NR > 1 {
                name = $2
                gsub(/^[[:space:]]+|[[:space:]]+$/, "", name)
                if (name == wanted) {
                    found = 1
                    exit
                }
            }
            END { exit(found ? 0 : 1) }
        '
}

if box_exists; then
    printf 'Riutilizzo la Distrobox esistente: %s\n' "$BOX_NAME"
else
    printf 'Creo la Distrobox %s da %s\n' "$BOX_NAME" "$BOX_IMAGE"
    distrobox create --yes --no-entry --name "$BOX_NAME" --image "$BOX_IMAGE"
fi

printf 'Configuro ambiente di sviluppo, locale e fuso orario...\n'

distrobox enter "$BOX_NAME" -- bash -s <<'EOF'
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

sudo apt-get update
sudo apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    cargo \
    clippy \
    git \
    gnupg \
    libwayland-dev \
    locales \
    pkg-config \
    rsync \
    rustc \
    rustfmt \
    tzdata

# Carta richiede Rust >= 1.85.
rust_version=$(rustc --version | awk '{print $2}')
if ! dpkg --compare-versions "$rust_version" ge 1.85; then
    printf 'setup-carta-distrobox: Rust %s è troppo vecchio; serve >= 1.85\n' \
        "$rust_version" >&2
    exit 1
fi

# Fuso orario.
sudo ln -snf /usr/share/zoneinfo/Europe/Rome /etc/localtime
printf '%s\n' 'Europe/Rome' | sudo tee /etc/timezone >/dev/null
sudo dpkg-reconfigure -f noninteractive tzdata >/dev/null

# Locale italiano UTF-8.
if grep -Eq '^#[[:space:]]*it_IT\.UTF-8[[:space:]]+UTF-8' /etc/locale.gen; then
    sudo sed -i 's/^#[[:space:]]*it_IT\.UTF-8[[:space:]]\+UTF-8/it_IT.UTF-8 UTF-8/' /etc/locale.gen
elif ! grep -Eq '^it_IT\.UTF-8[[:space:]]+UTF-8' /etc/locale.gen; then
    printf '%s\n' 'it_IT.UTF-8 UTF-8' | sudo tee -a /etc/locale.gen >/dev/null
fi
sudo locale-gen it_IT.UTF-8 >/dev/null
sudo update-locale LANG=it_IT.UTF-8

printf '\nAmbiente Carta pronto.\n'
printf 'Rust:   %s\n' "$(rustc --version)"
printf 'Cargo:  %s\n' "$(cargo --version)"
printf 'Locale: %s\n' "$(LANG=it_IT.UTF-8 locale charmap)"
printf 'Zona:   %s\n' "$(cat /etc/timezone)"
printf 'Ora:    %s\n' "$(TZ=Europe/Rome date '+%Y-%m-%d %H:%M:%S %Z')"
EOF

cat <<EOF

Distrobox pronta: $BOX_NAME

Carta Space non è stata installata nel container.
Dal repository puoi avviare la TUI con:

  scripts/carta-tui.sh

Per entrare manualmente nell'ambiente:

  distrobox enter $BOX_NAME
EOF
