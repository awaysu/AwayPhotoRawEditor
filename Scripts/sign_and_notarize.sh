#!/bin/bash
# Sign, notarize and staple AwayPhotoRawEditor.app, then package it as a DMG.
#
# Order matters and cannot be rearranged: the nested dylibs are signed before the bundle
# that contains them, the bundle is signed before it is put in a DMG, and the DMG is
# signed after it is built. Signing changes file contents, so any checksum you publish
# must be computed last of all.
#
# Usage:
#   Scripts/sign_and_notarize.sh "Developer ID Application: Your Name (TEAMID)" [keychain-profile]
#
# The keychain profile is created once with:
#   xcrun notarytool store-credentials awpr-notary \
#       --apple-id you@example.com --team-id TEAMID --password <app-specific-password>
#
# An app-specific password is generated at appleid.apple.com, not your Apple ID password.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$(pwd)
# Dedicated signing keychain (headless / SSH sessions) — defines cs() and nt().
. "$ROOT/Scripts/signing_keychain.sh"
IDENTITY=${1:-}
PROFILE=${2:-awpr-notary}
APP="$ROOT/build/AwayPhotoRawEditor.app"
ENTITLEMENTS="$ROOT/Resources/AwayPhotoRawEditor.entitlements"

if [ -z "$IDENTITY" ]; then
    echo "Usage: $0 \"Developer ID Application: Name (TEAMID)\" [keychain-profile]" >&2
    echo >&2
    echo "Available identities:" >&2
    security find-identity -v -p codesigning || true
    exit 1
fi

echo "==> 1/6  Rebuilding with the Developer ID identity"
SIGN_IDENTITY="$IDENTITY" ./Scripts/build_app.sh > /dev/null

# Read the version AFTER the rebuild — reading the old bundle first once produced a
# freshly-notarized 1.0.18 app inside a DMG named 1.0.0 (2026-09-01).
VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$APP/Contents/Info.plist")
DMG="$ROOT/build/AwayPhotoRawEditor-$VERSION.dmg"

echo "==> 2/6  Re-signing with hardened runtime + timestamp"
# --options runtime is what notarization requires. Inner code first, bundle last:
# signing the bundle seals what is inside it, so a later change to a nested dylib
# invalidates the outer signature.
for lib in "$APP"/Contents/Frameworks/*.dylib; do
    cs --force --options runtime --timestamp --sign "$IDENTITY" "$lib"
done
cs --force --options runtime --timestamp \
    --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP/Contents/MacOS/awpr-cli"
cs --force --options runtime --timestamp \
    --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP/Contents/MacOS/AwayPhotoRawEditor"
cs --force --options runtime --timestamp \
    --entitlements "$ENTITLEMENTS" --sign "$IDENTITY" "$APP"

echo "==> 3/6  Verifying the signature"
codesign --verify --deep --strict --verbose=2 "$APP"
# Gatekeeper's own view. Before notarization this reports "rejected" with
# "source=Unnotarized Developer ID" — that is expected at this step, not a failure.
spctl --assess --type execute --verbose=4 "$APP" 2>&1 || true

echo "==> 4/6  Submitting for notarization (this usually takes a few minutes)"
ZIP="$ROOT/build/AwayPhotoRawEditor-notarize.zip"
rm -f "$ZIP"
# ditto, not zip: it preserves the extended attributes and symlinks in the bundle.
ditto -c -k --keepParent "$APP" "$ZIP"

if ! nt submit "$ZIP" --keychain-profile "$PROFILE" --wait; then
    echo >&2
    echo "!! Notarization failed. To see why:" >&2
    echo "   xcrun notarytool history --keychain-profile $PROFILE" >&2
    echo "   xcrun notarytool log <submission-id> --keychain-profile $PROFILE" >&2
    exit 1
fi
rm -f "$ZIP"

echo "==> 5/6  Stapling"
# Stapling attaches the ticket to the app so it launches without a network round trip.
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"

echo "==> 6/6  Building the DMG"
rm -f "$DMG"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "AwayPhotoRawEditor" -srcfolder "$STAGE" \
    -ov -format UDZO "$DMG" > /dev/null
rm -rf "$STAGE"

# The DMG is signed and notarized in its own right, so the download itself is trusted
# rather than only the app inside it.
cs --force --timestamp --sign "$IDENTITY" "$DMG"
nt submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"

echo
echo "==> Done"
echo "    $DMG"
# Checksums come last: every signing step above changed the bytes.
echo "    SHA256: $(shasum -a 256 "$DMG" | awk '{print $1}')"
echo
echo "Verify a clean install with:"
echo "    spctl --assess --type execute --verbose=4 $APP"
echo "    xcrun stapler validate $DMG"
