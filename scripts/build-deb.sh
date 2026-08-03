#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_NAME="ghostab"
VERSION="$(grep '^version = ' "$ROOT_DIR/Cargo.toml" | head -n 1 | cut -d '"' -f 2)"
DIST_DIR="$ROOT_DIR/dist"

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: required command '$1' was not found" >&2
        exit 1
    fi
}

require_command cargo
require_command dpkg
require_command dpkg-deb

ARCH="${GHOSTAB_DEB_ARCH:-$(dpkg --print-architecture)}"
PACKAGE_ROOT="$DIST_DIR/${PACKAGE_NAME}_${VERSION}_${ARCH}"
DEB_PATH="$DIST_DIR/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"

rm -rf "$PACKAGE_ROOT"
mkdir -p "$PACKAGE_ROOT/DEBIAN"
mkdir -p "$PACKAGE_ROOT/usr/bin"
mkdir -p "$PACKAGE_ROOT/usr/share/applications"
mkdir -p "$PACKAGE_ROOT/usr/share/doc/$PACKAGE_NAME"
mkdir -p "$DIST_DIR"

cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

install -Dm755 "$ROOT_DIR/target/release/ghostab" "$PACKAGE_ROOT/usr/bin/ghostab"
install -Dm644 "$ROOT_DIR/packaging/linux/io.github.ghostab.Ghostab.desktop" \
    "$PACKAGE_ROOT/usr/share/applications/io.github.ghostab.Ghostab.desktop"
install -Dm644 "$ROOT_DIR/README.md" "$PACKAGE_ROOT/usr/share/doc/$PACKAGE_NAME/README.md"
install -Dm644 "$ROOT_DIR/LICENSE" "$PACKAGE_ROOT/usr/share/doc/$PACKAGE_NAME/copyright"
install -Dm644 "$ROOT_DIR/examples/hello.html" "$PACKAGE_ROOT/usr/share/doc/$PACKAGE_NAME/examples/hello.html"

sed \
    -e "s/^Version: .*/Version: $VERSION/" \
    -e "s/^Architecture: .*/Architecture: $ARCH/" \
    -e "s/^Maintainer: .*/Maintainer: AramCZ <aramcz@protonmail.com>/" \
    "$ROOT_DIR/packaging/debian/control" > "$PACKAGE_ROOT/DEBIAN/control"

dpkg-deb --root-owner-group --build "$PACKAGE_ROOT" "$DEB_PATH"

echo "Built $DEB_PATH"
