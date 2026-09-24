#!/bin/sh
# Installs AeternaVault from this folder for the current user (no root needed):
#
#   ./install.sh              install or update
#   ./install.sh --uninstall  remove programs, menu entry, icons and the
#                             background timer; settings and backups stay
#   ./install.sh --uninstall --purge
#                             also remove settings, history and the
#                             remembered key
#
# Programs go to ~/.local/bin, which most distributions put on PATH.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
config="${XDG_CONFIG_HOME:-$HOME/.config}"
apps="$data/applications"
icons="$data/icons/hicolor"
module="$data/powershell/Modules/AeternaVault"

uninstall() {
    if [ -x "$bin/aeternavault-cli" ]; then
        "$bin/aeternavault-cli" system cleanup || true
    fi
    rm -f "$bin/aeternavault" "$bin/aeternavault-cli" "$apps/aeternavault.desktop"
    rm -f "$icons/256x256/apps/aeternavault.png" "$icons/512x512/apps/aeternavault.png"
    rm -rf "$module"
    if [ "${1:-}" = "--purge" ]; then
        rm -rf "$config/aeternavault" "$data/aeternavault"
    fi
    echo "AeternaVault was removed."
}

case "${1:-}" in
    --uninstall)
        uninstall "${2:-}"
        exit 0
        ;;
    "") ;;
    *)
        sed -n '2,13p' "$0"
        exit 2
        ;;
esac

mkdir -p "$bin"
install -m 755 "$here/aeternavault-cli" "$bin/aeternavault-cli"
if [ -f "$here/aeternavault" ]; then
    install -m 755 "$here/aeternavault" "$bin/aeternavault"
    mkdir -p "$apps" "$icons/256x256/apps" "$icons/512x512/apps"
    sed "s|^Exec=aeternavault|Exec=$bin/aeternavault|" "$here/aeternavault.desktop" > "$apps/aeternavault.desktop"
    install -m 644 "$here/icons/aeternavault-256.png" "$icons/256x256/apps/aeternavault.png"
    install -m 644 "$here/icons/aeternavault-512.png" "$icons/512x512/apps/aeternavault.png"
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps" || true
fi
if [ -d "$here/powershell/AeternaVault" ]; then
    mkdir -p "$module"
    cp -R "$here/powershell/AeternaVault/." "$module/"
fi

echo "AeternaVault was installed to $bin."
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "Note: $bin is not on PATH. Add it, or call $bin/aeternavault-cli directly." ;;
esac
