#!/bin/bash
# Build LLVM's OpenMP runtime (libomp) into ThirdParty/libomp, universal and against
# LSMinimumSystemVersion — for the same reason Scripts/build_libraw.sh exists:
# Homebrew's libomp is arm64-only and carries the build machine's macOS as its minimum
# (measured: minos 26.0), so bundling it would raise the app's own minimum.
#
# LibRaw's demosaic (AHD/DHT), CR3 decoder, raw2image and postprocessing loops are
# `#pragma omp parallel`; without this runtime they run on one core and full-resolution
# decode is 1.4–2.2x slower than the C# port that links Homebrew's libomp.
#
# Needs cmake (brew install cmake). Apple clang compiles OpenMP with `-Xclang -fopenmp`
# and needs only omp.h + libomp.dylib from here.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
VERSION=${LLVM_VERSION:-20.1.8}
DEPLOY=${MACOSX_DEPLOYMENT_TARGET:-14.0}
PREFIX="$ROOT/ThirdParty/libomp"
WORK="$ROOT/build/libomp-src"

command -v cmake >/dev/null || { echo "!! cmake not found: brew install cmake" >&2; exit 1; }

echo "==> libomp from LLVM $VERSION  (deployment target $DEPLOY, universal)"
mkdir -p "$WORK"
BASE="https://github.com/llvm/llvm-project/releases/download/llvmorg-$VERSION"
for t in openmp cmake; do
    TAR="$WORK/$t-$VERSION.src.tar.xz"
    if [ ! -f "$TAR" ]; then
        echo "==> Downloading $t-$VERSION.src.tar.xz"
        curl -fL "$BASE/$t-$VERSION.src.tar.xz" -o "$TAR"
    fi
done

rm -rf "$WORK/openmp-$VERSION.src" "$WORK/cmake" "$WORK/cmake-$VERSION.src"
tar xf "$WORK/openmp-$VERSION.src.tar.xz" -C "$WORK"
tar xf "$WORK/cmake-$VERSION.src.tar.xz" -C "$WORK"
# The standalone openmp build looks for ../cmake/Modules from the monorepo layout.
mv "$WORK/cmake-$VERSION.src" "$WORK/cmake"
SRC="$WORK/openmp-$VERSION.src"

build_arch() {
    local arch="$1"
    local b="$WORK/build-$arch"
    rm -rf "$b"
    cmake -S "$SRC" -B "$b" -G "Unix Makefiles" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_OSX_ARCHITECTURES="$arch" \
        -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOY" \
        -DLIBOMP_ARCH="$( [ "$arch" = arm64 ] && echo aarch64 || echo x86_64 )" \
        -DOPENMP_STANDALONE_BUILD=ON \
        -DLIBOMP_ENABLE_SHARED=ON \
        -DLIBOMP_OMPT_SUPPORT=OFF \
        -DLIBOMP_OMPD_SUPPORT=OFF \
        -DOPENMP_ENABLE_LIBOMPTARGET=OFF \
        -DLIBOMP_INSTALL_ALIASES=OFF \
        -DCMAKE_INSTALL_PREFIX="$b/out" \
        > "$b.log" 2>&1 || { tail -30 "$b.log"; exit 1; }
    cmake --build "$b" --target install -j "$(sysctl -n hw.ncpu)" >> "$b.log" 2>&1 || { tail -30 "$b.log"; exit 1; }
}

echo "==> Building arm64"
build_arch arm64
echo "==> Building x86_64"
build_arch x86_64

rm -rf "$PREFIX"
mkdir -p "$PREFIX/lib" "$PREFIX/include"
lipo -create "$WORK/build-arm64/out/lib/libomp.dylib" "$WORK/build-x86_64/out/lib/libomp.dylib" \
     -output "$PREFIX/lib/libomp.dylib"
install_name_tool -id "@rpath/libomp.dylib" "$PREFIX/lib/libomp.dylib"
cp "$WORK/build-arm64/out/include/omp.h" "$PREFIX/include/"

LIB="$PREFIX/lib/libomp.dylib"
echo "==> Built $LIB"
lipo -info "$LIB"
otool -l "$LIB" | grep -A3 LC_BUILD_VERSION | grep minos || true
otool -L "$LIB" | tail -n +2 | grep -v '/usr/lib\|/System' || echo "    (no non-system dependencies)"
echo
echo "Now re-run Scripts/build_libraw.sh so LibRaw links against it."
