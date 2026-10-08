#!/usr/bin/env bash
# Linux release: release build -> .deb (cargo-deb) and .rpm (cargo-generate-rpm), each with
# the .desktop entry, the icon, LICENSE, the LibRaw CDDL text and THIRD-PARTY-NOTICES ->
# SHA256. Output in dist/. Only .deb and .rpm are published (no AppImage, 2026-10-05).
#
#   scripts/package-linux.sh
#
# Needs dpkg (dpkg-shlibdeps works out the library dependencies). The two cargo tools are
# installed on first use.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
DIST="$ROOT/dist"
echo "==> AwayPhotoRawEditor $VERSION"

# Packages want a "~" for a pre-release (2.0.0~dev sorts before 2.0.0; rpm rejects "-").
PKG_VERSION=${VERSION/-/\~}
cargo deb --version > /dev/null 2>&1 || cargo install --locked cargo-deb
cargo generate-rpm --version > /dev/null 2>&1 || cargo install --locked cargo-generate-rpm

echo "==> 1/4  cargo build --release"
cargo build --release -p awpr-app
mkdir -p "$DIST"

echo "==> 2/4  .deb"
cargo deb -p awpr-app --no-build --deb-version "$PKG_VERSION-1" --output "$DIST"

echo "==> 3/4  .rpm"
strip -o "$DIST/AwayPhotoRawEditor.stripped" target/release/AwayPhotoRawEditor
cp target/release/AwayPhotoRawEditor "$DIST/AwayPhotoRawEditor.unstripped"
cp "$DIST/AwayPhotoRawEditor.stripped" target/release/AwayPhotoRawEditor
cargo generate-rpm -p crates/app --set-metadata "version = \"$PKG_VERSION\"" --output "$DIST"
mv "$DIST/AwayPhotoRawEditor.unstripped" target/release/AwayPhotoRawEditor
rm -f "$DIST/AwayPhotoRawEditor.stripped"

echo "==> 4/4  contents + SHA256"
DEB=$(ls -t "$DIST"/awayphotoraweditor_*.deb | head -1)
RPM=$(ls -t "$DIST"/awayphotoraweditor-*.rpm | head -1)
dpkg-deb --info "$DEB" | sed -n '/Package:/,$p'
dpkg -c "$DEB"
if command -v rpm > /dev/null; then
    rpm -qip "$RPM"
    rpm -qlp "$RPM"
    rpm -qRp "$RPM"
elif command -v 7z > /dev/null && command -v zstd > /dev/null; then
    # No rpm tool here: the payload is a zstd-compressed cpio archive after the header.
    7z x -so "$RPM" 2>/dev/null | zstd -dc | cpio -itv 2>/dev/null || true
fi
ls -l "$DEB" "$RPM"
( cd "$DIST" && sha256sum "$(basename "$DEB")" "$(basename "$RPM")" > SHA256SUMS-linux.txt && cat SHA256SUMS-linux.txt )
echo "==> Done"
