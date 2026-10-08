#!/usr/bin/env bash
# macOS release: universal (arm64 + x86_64) .app -> sign -> notarize -> staple -> DMG ->
# sign / notarize / staple the DMG -> quarantine check -> SHA256. Output in dist/.
#
#   AWPR_SIGN_IDENTITY="Developer ID Application: Chih-Wei Su (BNH8YS88T9)" \
#   AWPR_NOTARY_PROFILE=AwayTerminalNotary \
#   scripts/package-macos.sh
#
# Without AWPR_SIGN_IDENTITY the .app is only ad-hoc signed and the DMG unsigned (it runs
# here, Gatekeeper blocks it elsewhere). Without AWPR_NOTARY_PROFILE it is signed but not
# notarized. Order cannot change: nested code before the bundle, the bundle notarized and
# stapled before it goes into the DMG, checksums last of all.
#
# Keychains: codesign only ever looks in the login keychain (AWPR_KEYCHAIN). Other
# keychains on the build Mac may be locked, and touching one stops at an unlock dialog.
# Every signing / notarization command is killed after AWPR_STEP_TIMEOUT seconds (180).
# Over SSH the login keychain is locked in that session (codesign: errSecInternalComponent),
# and an unlock from another session does not carry over: set AWPR_KEYCHAIN_PASSWORD to have
# this script unlock it first. The password only ever comes from the environment.
# AWPR_LIBRAW_OPENMP=0 builds LibRaw single-threaded without libomp (CI has no universal
# libomp; decoding is ~2.5x slower, the pixels are identical).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)

VERSION=$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
SHORT=${VERSION%%[-+]*}               # CFBundleShortVersionString must be numeric
IDENTITY=${AWPR_SIGN_IDENTITY:-}
PROFILE=${AWPR_NOTARY_PROFILE:-}
KEYCHAIN=${AWPR_KEYCHAIN:-$HOME/Library/Keychains/login.keychain-db}
STEP_TIMEOUT=${AWPR_STEP_TIMEOUT:-180}
OMP=${AWPR_LIBOMP_DIR:-$(cd .. && pwd)/AwayPhotoRawEditor_Swift/ThirdParty/libomp}
DIST="$ROOT/dist"
APP="$DIST/AwayPhotoRawEditor.app"
DMG="$DIST/AwayPhotoRawEditor-$VERSION-macOS.dmg"
ENT="$ROOT/assets/AwayPhotoRawEditor.entitlements"
echo "==> AwayPhotoRawEditor $VERSION (bundle $SHORT)"

# Run a command, killing it if it has not returned after STEP_TIMEOUT seconds.
bounded() {
    "$@" &
    local pid=$!
    ( sleep "$STEP_TIMEOUT"; kill "$pid" 2>/dev/null && echo "!! killed after ${STEP_TIMEOUT}s: $*" >&2 ) &
    local watcher=$!
    local rc=0
    wait "$pid" || rc=$?
    kill "$watcher" 2>/dev/null || true
    wait "$watcher" 2>/dev/null || true
    return "$rc"
}

sign() { # sign <path> [extra codesign args]
    local path=$1; shift
    if [ -n "$IDENTITY" ]; then
        bounded codesign --force --options runtime --timestamp --keychain "$KEYCHAIN" --sign "$IDENTITY" "$@" "$path"
    else
        codesign --force --sign - "$@" "$path"   # ad-hoc: required to run on Apple silicon
    fi
}

notarize() { # notarize <file>
    [ -n "$IDENTITY" ] && [ -n "$PROFILE" ] || return 0
    bounded xcrun notarytool submit "$1" --keychain-profile "$PROFILE" --wait
}

if [ -n "$IDENTITY" ] && [ -n "${AWPR_KEYCHAIN_PASSWORD:-}" ]; then
    echo "    unlocking $KEYCHAIN (AWPR_KEYCHAIN_PASSWORD)"
    security unlock-keychain -p "$AWPR_KEYCHAIN_PASSWORD" "$KEYCHAIN"
fi

USE_OMP=1
[ "${AWPR_LIBRAW_OPENMP:-1}" = 0 ] && USE_OMP=0
if [ "$USE_OMP" = 1 ]; then
    [ -f "$OMP/lib/libomp.dylib" ] || { echo "libomp not found under $OMP (see scripts/build-macos.sh, or AWPR_LIBRAW_OPENMP=0)" >&2; exit 1; }
    case "$(lipo -archs "$OMP/lib/libomp.dylib")" in
        *x86_64*arm64*|*arm64*x86_64*) ;;
        *) echo "$OMP/lib/libomp.dylib must be universal" >&2; exit 1 ;;
    esac
    export AWPR_LIBOMP_DIR="$OMP"
else
    echo "    LibRaw without OpenMP (AWPR_LIBRAW_OPENMP=0)"
fi

# ---- 1. both architectures, then one universal binary ----
echo "==> 1/7  build arm64 + x86_64"
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    rustup target list --installed | grep -qx "$t" || rustup target add "$t"
done
# The bundle loads libomp from Contents/Frameworks.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-rpath,@executable_path/../Frameworks"
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-14.0}   # libomp is built for 14
for t in aarch64-apple-darwin x86_64-apple-darwin; do
    cargo build --release -p awpr-app --target "$t"
done
mkdir -p "$DIST"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Frameworks" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/AwayPhotoRawEditor" \
    target/aarch64-apple-darwin/release/AwayPhotoRawEditor target/x86_64-apple-darwin/release/AwayPhotoRawEditor

# ---- 2. the bundle ----
echo "==> 2/7  .app"
[ "$USE_OMP" = 1 ] && cp "$OMP/lib/libomp.dylib" "$APP/Contents/Frameworks/"
cp assets/AppIcon.icns "$APP/Contents/Resources/"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp THIRD-PARTY-NOTICES.md CHANGELOG.md "$APP/Contents/Resources/"
cp crates/libraw-sys/vendor/LibRaw-0.22.2/LICENSE.CDDL "$APP/Contents/Resources/LibRaw-LICENSE.CDDL.txt"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>AwayPhotoRawEditor</string>
    <key>CFBundleDisplayName</key><string>AwayPhotoRawEditor</string>
    <key>CFBundleIdentifier</key><string>com.awaysu.awayphotoraweditor</string>
    <key>CFBundleExecutable</key><string>AwayPhotoRawEditor</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$SHORT</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>$MACOSX_DEPLOYMENT_TARGET</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.photography</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Copyright (c) 2026 Chih-Wei Su (Awaysu)</string>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" > /dev/null

# ---- 3. sign: nested code first, the bundle last ----
echo "==> 3/7  sign ${IDENTITY:-(ad-hoc)}"
[ "$USE_OMP" = 1 ] && sign "$APP/Contents/Frameworks/libomp.dylib"
sign "$APP/Contents/MacOS/AwayPhotoRawEditor" --entitlements "$ENT"
sign "$APP" --entitlements "$ENT"
codesign --verify --deep --strict --verbose=2 "$APP"

# ---- 4. notarize + staple the app ----
if [ -n "$IDENTITY" ] && [ -n "$PROFILE" ]; then
    echo "==> 4/7  notarize .app ($PROFILE)"
    ZIP="$DIST/AwayPhotoRawEditor-notarize.zip"
    rm -f "$ZIP"
    ditto -c -k --keepParent "$APP" "$ZIP"   # ditto keeps the bundle's metadata; zip does not
    notarize "$ZIP"
    rm -f "$ZIP"
    xcrun stapler staple "$APP"
    xcrun stapler validate "$APP"
else
    echo "==> 4/7  notarize .app: skipped (no ${IDENTITY:+AWPR_NOTARY_PROFILE}${IDENTITY:-AWPR_SIGN_IDENTITY})"
fi

# ---- 5. DMG: the app + an Applications link ----
echo "==> 5/7  DMG"
rm -f "$DMG"
STAGE=$(mktemp -d)
ditto "$APP" "$STAGE/AwayPhotoRawEditor.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname AwayPhotoRawEditor -srcfolder "$STAGE" -ov -format UDZO "$DMG" > /dev/null
rm -rf "$STAGE"

# ---- 6. sign / notarize / staple the DMG ----
if [ -n "$IDENTITY" ]; then
    echo "==> 6/7  sign DMG"
    bounded codesign --force --timestamp --keychain "$KEYCHAIN" --sign "$IDENTITY" "$DMG"
    if [ -n "$PROFILE" ]; then
        notarize "$DMG"
        xcrun stapler staple "$DMG"
    fi
else
    echo "==> 6/7  DMG left unsigned"
fi

# ---- 7. the user's view (a quarantined download), then checksums ----
echo "==> 7/7  quarantine check + SHA256"
lipo -archs "$APP/Contents/MacOS/AwayPhotoRawEditor"
Q="$DIST/quarantine-check.dmg"
cp "$DMG" "$Q"
xattr -w com.apple.quarantine "0083;$(printf '%08x' "$(date +%s)");Safari;$(uuidgen)" "$Q"
spctl -a -vvv -t install "$Q" 2>&1 || true
spctl --assess --type execute --verbose=4 "$APP" 2>&1 || true
rm -f "$Q"
( cd "$DIST" && shasum -a 256 "$(basename "$DMG")" > SHA256SUMS-macos.txt && cat SHA256SUMS-macos.txt )
echo "==> Done: $DMG"
