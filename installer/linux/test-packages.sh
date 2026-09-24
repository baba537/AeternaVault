#!/bin/sh
# Installs, updates and removes the Linux packages on a CI machine.
# Changes the system: runs only in CI.
#
#   installer/linux/test-packages.sh [previous.deb]
set -eu

[ "${CI:-}" = "true" ] || { echo "This test changes the system; it only runs in CI." >&2; exit 1; }
cd "$(dirname "$0")/../.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
gui_deb=$(ls dist/aeternavault_"$version"-*_amd64.deb)
cli_deb=$(ls dist/aeternavault-cli_"$version"-*_amd64.deb)

ok() { echo "ok  $1"; }
fail() { echo "FAILED: $1" >&2; exit 1; }

if [ $# -ge 1 ] && [ -f "$1" ]; then
    sudo apt-get install -y "./$1" >/dev/null
    ok "previous version installed"
fi

# Settings from before the update must survive it.
export AETERNAVAULT_HOME="$RUNNER_TEMP/av-home"
sudo apt-get install -y "./$gui_deb" >/dev/null
aeternavault-cli --version >/dev/null || fail "aeternavault-cli runs"
aeternavault --version >/dev/null || fail "aeternavault runs"
[ -f /usr/share/applications/aeternavault.desktop ] || fail "menu entry installed"
ok "window package installed"
aeternavault-cli config set advanced.hardlink_unchanged false >/dev/null
sudo apt-get install -y --reinstall "./$gui_deb" >/dev/null
[ "$(aeternavault-cli config get advanced.hardlink_unchanged)" = "false" ] || fail "settings kept after the update"
ok "settings kept after the update"

# The command-line package replaces the window package and vice versa.
sudo apt-get install -y "./$cli_deb" >/dev/null
aeternavault-cli --version >/dev/null || fail "command-line package runs"
[ ! -e /usr/bin/aeternavault ] || fail "window program removed by the command-line package"
ldd /usr/bin/aeternavault-cli | grep -qE 'libX11|libwayland|libGL' && fail "command line needs no window libraries"
ok "command-line package installed"

sudo apt-get remove -y aeternavault-cli >/dev/null
[ ! -e /usr/bin/aeternavault-cli ] || fail "package removed"
ok "package removed"

# User installation from the tarball.
tmp=$(mktemp -d)
tar -C "$tmp" -xzf "dist/aeternavault-$version-linux-x64.tar.gz"
HOME="$tmp/home" sh "$tmp/aeternavault-$version-linux-x64/install.sh" >/dev/null
[ -x "$tmp/home/.local/bin/aeternavault-cli" ] || fail "tarball installs the command line"
[ -f "$tmp/home/.local/share/applications/aeternavault.desktop" ] || fail "tarball installs the menu entry"
HOME="$tmp/home" sh "$tmp/aeternavault-$version-linux-x64/install.sh" --uninstall >/dev/null
[ ! -e "$tmp/home/.local/bin/aeternavault-cli" ] || fail "tarball uninstall"
[ ! -e "$tmp/home/.local/share/applications/aeternavault.desktop" ] || fail "tarball uninstall removes the menu entry"
rm -rf "$tmp"
ok "tarball install and uninstall"
echo "Package checks passed."
