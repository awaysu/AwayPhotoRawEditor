#!/usr/bin/env bash
# macOS release build with an OpenMP LibRaw.
#
# Apple clang has no OpenMP runtime. Without one, LibRaw builds single-threaded: the pixels
# are identical but a full decode is ~2.5x slower (1220 ms vs 470 ms on an M2 for 24 MP).
# This links the universal, minos-14 libomp.dylib that AwayPhotoRawEditor_Swift builds with
# Scripts/build_libomp.sh, i.e. the same runtime the shipping Swift app bundles.
#
#   scripts/build-macos.sh [path/to/libomp-prefix]   (default: ../AwayPhotoRawEditor_Swift/ThirdParty/libomp)
set -euo pipefail
cd "$(dirname "$0")/.."
OMP=${1:-$(cd .. && pwd)/AwayPhotoRawEditor_Swift/ThirdParty/libomp}
[ $# -gt 0 ] && shift
[ -f "$OMP/lib/libomp.dylib" ] && [ -f "$OMP/include/omp.h" ] || {
    echo "libomp not found under $OMP (run AwayPhotoRawEditor_Swift/Scripts/build_libomp.sh)" >&2
    exit 1
}
export AWPR_LIBOMP_DIR="$OMP"
# Development rpath; a bundled .app would use @executable_path/../Frameworks instead.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,$OMP/lib"
cargo build --release "$@"
