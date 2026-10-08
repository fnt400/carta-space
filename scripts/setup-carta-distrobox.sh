#!/usr/bin/env bash
set -euo pipefail

# Prepara la Distrobox di sviluppo di Carta Space.
# Carta non viene installata nel container: il repository resta nel normale
# $HOME condiviso da Distrobox. La GUI si avvia con cargo dal repository.
#
# Variabili opzionali:
#   CARTA_DISTROBOX_NAME   nome del container (default: carta-gui-dev)
#   CARTA_DISTROBOX_IMAGE  immagine Distrobox (default: ubuntu:26.04)

BOX_NAME="${CARTA_DISTROBOX_NAME:-carta-gui-dev}"
BOX_IMAGE="${CARTA_DISTROBOX_IMAGE:-ubuntu:26.04}"

host_timezone() {
    if [[ -n "${TZ:-}" ]]; then
        printf '%s\n' "$TZ"
        return
    fi

    if command -v timedatectl >/dev/null 2>&1; then
        local zone
        zone="$(timedatectl show -p Timezone --value 2>/dev/null || true)"
        if [[ -n "$zone" ]]; then
            printf '%s\n' "$zone"
            return
        fi
    fi

    local target
    target="$(readlink -f /etc/localtime 2>/dev/null || true)"
    if [[ "$target" == *"/zoneinfo/"* ]]; then
        printf '%s\n' "${target#*/zoneinfo/}"
    fi
}

HOST_TIMEZONE="$(host_timezone)"
HOST_LOCALE="${LC_ALL:-${LC_TIME:-${LANG:-C.UTF-8}}}"

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

printf 'Configuro ambiente di sviluppo...\n'

distrobox enter "$BOX_NAME" -- env \
    CARTA_HOST_TIMEZONE="$HOST_TIMEZONE" \
    CARTA_HOST_LOCALE="$HOST_LOCALE" \
    bash -s <<'EOF'
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive

sudo apt-get update
sudo apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    cargo \
    rust-clippy \
    git \
    gnupg \
    libwayland-dev \
    locales \
    pkg-config \
    rsync \
    rustc \
    rustfmt \
    tzdata

# Il frontend GUI v0.2 richiede Rust >= 1.88.
rust_version=$(rustc --version | awk '{print $2}')
if ! dpkg --compare-versions "$rust_version" ge 1.88; then
    printf 'setup-carta-distrobox: Rust %s è troppo vecchio; serve >= 1.88\n' \
        "$rust_version" >&2
    exit 1
fi

# Match the host timezone when the container has the corresponding zoneinfo data.
if [[ -n "${CARTA_HOST_TIMEZONE:-}" && -e "/usr/share/zoneinfo/$CARTA_HOST_TIMEZONE" ]]; then
    sudo ln -snf "/usr/share/zoneinfo/$CARTA_HOST_TIMEZONE" /etc/localtime
    printf '%s\n' "$CARTA_HOST_TIMEZONE" | sudo tee /etc/timezone >/dev/null
    sudo dpkg-reconfigure -f noninteractive tzdata >/dev/null
fi

# Generate support for the host locale without changing the container default.
host_locale="${CARTA_HOST_LOCALE:-}"
locale_key="${host_locale%%.*}"
if [[ -n "$locale_key" && "$locale_key" != "C" && "$locale_key" != "POSIX" ]]; then
    supported="$(awk -v key="$locale_key" '
        $1 ~ ("^" key "\\.") && $2 == "UTF-8" { print; exit }
    ' /usr/share/i18n/SUPPORTED)"
    if [[ -n "$supported" ]] && ! grep -Fxq "$supported" /etc/locale.gen; then
        printf '%s\n' "$supported" | sudo tee -a /etc/locale.gen >/dev/null
        sudo locale-gen >/dev/null
    fi
fi

printf '\nAmbiente Carta pronto.\n'
printf 'Rust:   %s\n' "$(rustc --version)"
printf 'Cargo:  %s\n' "$(cargo --version)"
printf 'Host locale: %s\n' "${CARTA_HOST_LOCALE:-system default}"
printf 'Host timezone: %s\n' "${CARTA_HOST_TIMEZONE:-system default}"
printf 'Container time: %s\n' "$(date '+%Y-%m-%d %H:%M:%S %Z %z')"
EOF

cat <<EOF

Distrobox pronta: $BOX_NAME

Carta Space non è stata installata nel container.
Dal repository puoi verificare il workspace e la GUI con:

  scripts/verify-carta.sh

Per entrare manualmente nell'ambiente:

  distrobox enter $BOX_NAME
EOF
