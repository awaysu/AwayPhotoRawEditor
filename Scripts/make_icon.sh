#!/bin/bash
# Build Resources/AppIcon.icns from Resources/icon.png.
# The counterpart of the Windows build's make_icon.ps1 — drop a new icon.png in and rerun.
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="Resources/icon.png"
OUT="Resources/AppIcon.icns"
[ -f "$SRC" ] || { echo "!! $SRC not found" >&2; exit 1; }

SET=$(mktemp -d)/AppIcon.iconset
mkdir -p "$SET"
# macOS wants each size at 1x and 2x; sips resamples from the single source.
for size in 16 32 128 256 512; do
    sips -z $size $size "$SRC" --out "$SET/icon_${size}x${size}.png" > /dev/null
    sips -z $((size*2)) $((size*2)) "$SRC" --out "$SET/icon_${size}x${size}@2x.png" > /dev/null
done
iconutil -c icns "$SET" -o "$OUT"
rm -rf "$(dirname "$SET")"
echo "==> Wrote $OUT"
