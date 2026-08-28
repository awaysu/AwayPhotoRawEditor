#!/bin/bash
# Build AwayPhotoRawEditor.app.
#
# SwiftPM produces a bare executable; a GUI app needs a bundle, and a distributable one
# needs LibRaw carried inside it (a user will not have Homebrew). This script assembles
# the bundle, copies libraw and its dependencies in, and rewrites their install names so
# nothing points back at /opt/homebrew.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
CONFIG=${CONFIG:-release}
APP="$ROOT/build/AwayPhotoRawEditor.app"
VERSION=$(grep -m1 'static let version' Sources/AwayPhotoRawEditor/UI/Dialogs.swift \
          | sed 's/.*"v\(.*\)".*/\1/')
BUILD_NUMBER=${BUILD_NUMBER:-$(date +%Y%m%d%H%M)}

echo "==> Building ($CONFIG, universal)"
# Both architectures: the bundled LibRaw is universal, and shipping an arm64-only
# executable beside it would silently drop Intel Macs. UNIVERSAL=0 builds host-only,
# which is faster while iterating.
ARCHS=""
if [ "${UNIVERSAL:-1}" = "1" ]; then
    ARCHS="--arch arm64 --arch x86_64"
fi
swift build -c "$CONFIG" $ARCHS --product AwayPhotoRawEditor
swift build -c "$CONFIG" $ARCHS --product awpr-cli
BIN=$(swift build -c "$CONFIG" $ARCHS --show-bin-path)

echo "==> Assembling bundle"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
cp "$BIN/AwayPhotoRawEditor" "$APP/Contents/MacOS/"
cp "$BIN/awpr-cli" "$APP/Contents/MacOS/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>AwayPhotoRawEditor</string>
    <key>CFBundleDisplayName</key><string>AwayPhotoRawEditor</string>
    <key>CFBundleIdentifier</key><string>cc.awaysu.AwayPhotoRawEditor</string>
    <key>CFBundleExecutable</key><string>AwayPhotoRawEditor</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$BUILD_NUMBER</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>LSMinimumSystemVersion</key><string>14.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2026 Chih-Wei Su (Awaysu). BSD 3-Clause.</string>
    <key>NSDesktopFolderUsageDescription</key>
    <string>匯出照片時需要存取桌面。</string>
    <key>NSDocumentsFolderUsageDescription</key>
    <string>開啟與匯出照片時需要存取文件資料夾。</string>
    <key>NSDownloadsFolderUsageDescription</key>
    <string>開啟與匯出照片時需要存取下載資料夾。</string>
    <key>NSPhotoLibraryUsageDescription</key>
    <string>讀取照片圖庫中的影像。</string>
    <key>CFBundleDocumentTypes</key>
    <array>
      <dict>
        <key>CFBundleTypeName</key><string>Camera RAW image</string>
        <key>CFBundleTypeRole</key><string>Editor</string>
        <key>LSItemContentTypes</key>
        <array>
          <string>public.camera-raw-image</string>
          <string>public.jpeg</string>
          <string>public.png</string>
          <string>public.tiff</string>
        </array>
      </dict>
    </array>
</dict>
</plist>
PLIST

# ---- bundle LibRaw ---------------------------------------------------------
# Copy libraw plus every non-system dylib it pulls in, then rewrite the install names so
# the app resolves them from its own Frameworks directory.
echo "==> Bundling LibRaw"
FW="$APP/Contents/Frameworks"

collect_deps() {
    local lib="$1"
    otool -L "$lib" | tail -n +2 | awk '{print $1}' | while read -r dep; do
        case "$dep" in
            /usr/lib/*|/System/*|@rpath/*|@executable_path/*|@loader_path/*) continue ;;
        esac
        local base
        base=$(basename "$dep")
        if [ ! -f "$FW/$base" ]; then
            cp -L "$dep" "$FW/$base"
            chmod u+w "$FW/$base"
            collect_deps "$FW/$base"
        fi
    done
}

# Prefer a locally built LibRaw: Homebrew's is compiled for whatever macOS the machine
# runs, so bundling it would raise the app's minimum system to that same version.
# Scripts/build_libraw.sh builds one against LSMinimumSystemVersion instead.
LIBRAW=""
for cand in "$ROOT"/ThirdParty/libraw/lib/libraw.*.dylib \
            /opt/homebrew/opt/libraw/lib/libraw.*.dylib \
            /usr/local/opt/libraw/lib/libraw.*.dylib; do
    # Skip the unversioned symlink so the real file is what gets copied.
    case "$cand" in *libraw.dylib) continue ;; esac
    if [ -f "$cand" ]; then LIBRAW="$cand"; break; fi
done
if [ -z "$LIBRAW" ]; then
    echo "!! libraw not found — run Scripts/build_libraw.sh, or: brew install libraw" >&2
    exit 1
fi
echo "    using $LIBRAW"
cp -L "$LIBRAW" "$FW/$(basename "$LIBRAW")"
chmod u+w "$FW/$(basename "$LIBRAW")"
collect_deps "$FW/$(basename "$LIBRAW")"

# Rewrite every id and cross-reference to @rpath.
for lib in "$FW"/*.dylib; do
    base=$(basename "$lib")
    install_name_tool -id "@rpath/$base" "$lib"
    otool -L "$lib" | tail -n +2 | awk '{print $1}' | while read -r dep; do
        depbase=$(basename "$dep")
        if [ -f "$FW/$depbase" ] && [ "$depbase" != "$base" ]; then
            install_name_tool -change "$dep" "@rpath/$depbase" "$lib"
        fi
    done
done

# Point both executables at the bundled copies.
for exe in "$APP/Contents/MacOS/AwayPhotoRawEditor" "$APP/Contents/MacOS/awpr-cli"; do
    install_name_tool -add_rpath "@executable_path/../Frameworks" "$exe" 2>/dev/null || true
    otool -L "$exe" | tail -n +2 | awk '{print $1}' | while read -r dep; do
        depbase=$(basename "$dep")
        if [ -f "$FW/$depbase" ]; then
            install_name_tool -change "$dep" "@rpath/$depbase" "$exe"
        fi
    done
done

# ---- sign ------------------------------------------------------------------
# Rewriting install names invalidates the signature every Homebrew dylib ships with, and
# on Apple Silicon an invalidly signed binary is killed at launch (SIGKILL, "Code
# Signature Invalid"). So everything gets re-signed here. Without SIGN_IDENTITY this is
# an ad-hoc signature, which is enough to run locally; Scripts/sign_and_notarize.sh
# re-signs with a Developer ID for distribution.
IDENTITY=${SIGN_IDENTITY:--}
echo "==> Signing (identity: $IDENTITY)"
for lib in "$FW"/*.dylib; do
    codesign --force --timestamp=none --sign "$IDENTITY" "$lib"
done
codesign --force --timestamp=none --sign "$IDENTITY" "$APP/Contents/MacOS/awpr-cli"
codesign --force --timestamp=none --sign "$IDENTITY" "$APP/Contents/MacOS/AwayPhotoRawEditor"

# ---- icon ------------------------------------------------------------------
if [ -f "$ROOT/Resources/AppIcon.icns" ]; then
    cp "$ROOT/Resources/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
elif [ -f "$ROOT/Resources/icon.png" ]; then
    echo "==> Generating icon"
    "$ROOT/Scripts/make_icon.sh"
    cp "$ROOT/Resources/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
fi

echo "==> Sealing bundle"
codesign --force --timestamp=none --sign "$IDENTITY" "$APP"
codesign --verify --deep --strict "$APP" && echo "    signature OK"

echo "==> Built $APP  (version $VERSION build $BUILD_NUMBER)"
otool -L "$APP/Contents/MacOS/AwayPhotoRawEditor" | grep -E 'raw|rpath' || true
