#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_ID="io.github.aramcz.ghostab"
VERSION="$(grep '^version = ' "$ROOT_DIR/Cargo.toml" | head -n 1 | cut -d '"' -f 2)"
ARCH="${GHOSTAB_FLATPAK_ARCH:-$(uname -m)}"
MANIFEST="$ROOT_DIR/packaging/flatpak/io.github.aramcz.ghostab.json"
DIST_DIR="$ROOT_DIR/dist"
STAGING_DIR="$DIST_DIR/flatpak-staging"
SRC_TARBALL="$DIST_DIR/ghostab-flatpak-src.tar.gz"
REPO_DIR="$DIST_DIR/flatpak-repo"
BUILD_DIR="$DIST_DIR/flatpak-build"
BUNDLE="$DIST_DIR/ghostab_${VERSION}_${ARCH}.flatpak"

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: required command '$1' was not found" >&2
        exit 1
    fi
}

require_command flatpak
require_command flatpak-builder

SDK_VERSION="$(grep '"runtime-version"' "$MANIFEST" | cut -d '"' -f 4)"
if ! flatpak info "org.freedesktop.Sdk.Extension.rust-stable/$ARCH/$SDK_VERSION" >/dev/null 2>&1; then
    echo "error: org.freedesktop.Sdk.Extension.rust-stable (branch $SDK_VERSION) is not installed" >&2
    echo "install it with: flatpak install flathub org.freedesktop.Sdk.Extension.rust-stable//$SDK_VERSION" >&2
    exit 1
fi

mkdir -p "$DIST_DIR"

echo "Staging source tree..."
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
tar -C "$ROOT_DIR" \
    --exclude=target \
    --exclude=dist \
    --exclude=.git \
    --exclude=packaging/debian/ghostab \
    --exclude='*.deb' \
    -cf - . | tar -C "$STAGING_DIR" -xf -
tar -C "$STAGING_DIR" -czf "$SRC_TARBALL" .

echo "Building Flatpak (this may take a while)..."
rm -rf "$REPO_DIR"
flatpak-builder \
    --default-branch=stable \
    --repo="$REPO_DIR" \
    --force-clean \
    --disable-rofiles-fuse \
    --arch="$ARCH" \
    "$BUILD_DIR" \
    "$MANIFEST"

echo "Bundling .flatpak..."
flatpak build-bundle "$REPO_DIR" "$BUNDLE" "$APP_ID" stable

echo "Built $BUNDLE"
