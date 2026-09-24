#!/bin/sh
# Builds the Linux downloads into dist/ (needs cargo-deb):
#   aeternavault_<version>-1_amd64.deb        window and command line
#   aeternavault-cli_<version>-1_amd64.deb    command line only
#   aeternavault-<version>-linux-x64.tar.gz     window and command line, install.sh
#   aeternavault-cli-<version>-linux-x64.tar.gz command line only, install.sh
set -eu

cd "$(dirname "$0")/../.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
mkdir -p dist

tarball() {
    name=$1
    shift
    staging=$(mktemp -d)
    dir="$staging/$name"
    mkdir -p "$dir/icons" "$dir/docs" "$dir/powershell"
    for file in "$@"; do
        install -m 755 "$file" "$dir/"
    done
    install -m 755 installer/linux/install.sh "$dir/"
    if [ -f "$dir/aeternavault" ]; then
        install -m 644 installer/linux/aeternavault.desktop "$dir/"
        install -m 644 assets/icon/aeternavault-256.png assets/icon/aeternavault-512.png "$dir/icons/"
    else
        rmdir "$dir/icons"
    fi
    cp -R powershell/AeternaVault "$dir/powershell/"
    install -m 644 README.md LICENSE-MIT LICENSE-APACHE "$dir/"
    install -m 644 docs/ENCRYPTION.md docs/cli-linux.md "$dir/docs/"
    install -m 644 tools/aeterna-decrypt.py "$dir/"
    tar -C "$staging" -czf "dist/$name.tar.gz" "$name"
    rm -rf "$staging"
}

# With the window. cargo deb builds target/release with the default features.
cargo deb --locked --output dist/
tarball "aeternavault-$version-linux-x64" target/release/aeternavault target/release/aeternavault-cli

# Command line only: a separate build without the window libraries.
CARGO_TARGET_DIR=target/cli cargo deb --locked --variant cli --output dist/
tarball "aeternavault-cli-$version-linux-x64" target/cli/release/aeternavault-cli

ls -l dist
