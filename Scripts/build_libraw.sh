#!/bin/bash
# Build LibRaw from source into ThirdParty/libraw.
#
# Why not just bundle Homebrew's: Homebrew compiles for whatever macOS the build machine
# runs, so its dylib carries that as its minimum. Shipping it would silently raise the
# app's own minimum to the same version. This builds a universal (arm64 + x86_64) LibRaw
# against LSMinimumSystemVersion instead.
#
# Version is pinned to match what the Windows build bundles, so a photo decodes to the
# same pixels on both platforms.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
VERSION=${LIBRAW_VERSION:-0.22.2}
DEPLOY=${MACOSX_DEPLOYMENT_TARGET:-14.0}
PREFIX="$ROOT/ThirdParty/libraw"
WORK="$ROOT/build/libraw-src"

echo "==> LibRaw $VERSION  (deployment target $DEPLOY, universal)"

mkdir -p "$WORK"
TARBALL="$WORK/LibRaw-$VERSION.tar.gz"
if [ ! -f "$TARBALL" ]; then
    echo "==> Downloading"
    curl -fL "https://www.libraw.org/data/LibRaw-$VERSION.tar.gz" -o "$TARBALL"
fi

SRC="$WORK/LibRaw-$VERSION"
rm -rf "$SRC"
tar xzf "$TARBALL" -C "$WORK"

cd "$SRC"
rm -rf "$PREFIX"
mkdir -p "$PREFIX/lib" "$PREFIX/include"

# Compiled directly rather than through configure: LibRaw's autotools path needs
# pkg-config, and going straight to clang also lets a single command emit a universal
# binary with an explicit deployment target — which is the whole point of building it
# here instead of using Homebrew's.
export MACOSX_DEPLOYMENT_TARGET="$DEPLOY"

SOVER=$(echo "$VERSION" | awk -F. '{printf "%d", $2+3}')   # 0.22.x -> libraw.25.dylib
DYLIB="libraw.$SOVER.dylib"

# OpenMP: LibRaw's demosaic (AHD/DHT), CR3 decoder, raw2image and postprocessing are
# `#pragma omp parallel`. Measured against the C# port (which links Homebrew's libomp),
# building without it made full-resolution decode 1.4–2.2x slower — one core instead
# of three. Scripts/build_libomp.sh produces a universal, minos-14 libomp; when it is
# present LibRaw links it and the .app bundles (and signs) one more dylib. Without it
# the build falls back to LIBRAW_NOTHREADS, as it was originally.
# The optional decoders we do not use are compiled out too.
OMP="$ROOT/ThirdParty/libomp"
DEFINES="-DNDEBUG -DNO_JASPER -DNO_JPEG -DNO_LCMS -DUSE_ZLIB"
OMP_CFLAGS=""
OMP_LDFLAGS=""
if [ -f "$OMP/lib/libomp.dylib" ] && [ -f "$OMP/include/omp.h" ]; then
    echo "==> OpenMP: linking $OMP/lib/libomp.dylib"
    OMP_CFLAGS="-Xclang -fopenmp -I$OMP/include"
    OMP_LDFLAGS="-L$OMP/lib -lomp"
else
    echo "==> OpenMP: ThirdParty/libomp not found, building single-threaded (run Scripts/build_libomp.sh)"
    DEFINES="$DEFINES -DLIBRAW_NOTHREADS"
fi

echo "==> Compiling (this takes a few minutes)"
# *_ph.cpp are placeholder translation units for a build *without* postprocessing —
# they redefine dcraw_process and friends, so including them alongside the real sources
# is a duplicate-symbol error. We need postprocessing, so they are excluded.
SOURCES=$(find src -name '*.cpp' ! -name '*_ph.cpp' | sort)
mkdir -p "$WORK/obj"

clang++ -shared -o "$PREFIX/lib/$DYLIB" \
    -arch arm64 -arch x86_64 \
    -mmacosx-version-min="$DEPLOY" \
    -O3 -fPIC -std=c++11 -w \
    $DEFINES $OMP_CFLAGS \
    -I. -Ilibraw \
    $SOURCES \
    -lz -lstdc++ $OMP_LDFLAGS \
    -install_name "@rpath/$DYLIB" \
    -compatibility_version "$SOVER.0.0" \
    -current_version "$SOVER.0.0" \
    > "$WORK/compile.log" 2>&1 || { tail -40 "$WORK/compile.log"; exit 1; }

cp -R libraw "$PREFIX/include/"
ln -sf "$DYLIB" "$PREFIX/lib/libraw.dylib"
# libraw references @rpath/libomp.dylib; keep a copy beside it so the single rpath the
# package and the .app already use resolves both.
if [ -n "$OMP_LDFLAGS" ]; then cp "$OMP/lib/libomp.dylib" "$PREFIX/lib/libomp.dylib"; fi

LIB="$PREFIX/lib/$DYLIB"
echo "==> Built $LIB"
lipo -info "$LIB"
otool -l "$LIB" | grep -A3 LC_BUILD_VERSION | grep minos || true
otool -L "$LIB" | tail -n +2 | grep -v '/usr/lib\|/System' || echo "    (no non-system dependencies)"
echo
echo "Scripts/build_app.sh will now prefer this build over Homebrew's."
