#!/bin/bash
# One-time setup of the dedicated signing keychain (see Scripts/signing_keychain.sh).
#
# ⚠️ Run this in Terminal.app — a GUI session where the login keychain is unlocked —
# because it copies the Developer ID identity *out of* the login keychain. Expect one or
# two keychain dialogs; click 允許 / Always Allow. It asks for the Apple app-specific
# password (appleid.apple.com → App-Specific Passwords) to store the notarytool profile.
#
# Re-running is safe: an existing keychain / password file is reused, the identity is
# re-imported (duplicates are ignored) and the notary profile overwritten.
set -euo pipefail

KC="$HOME/Library/Keychains/awpr-signing.keychain-db"
PASSFILE="$HOME/.config/awpr/signing-keychain.pass"
LOGIN="$HOME/Library/Keychains/login.keychain-db"
IDENTITY_NAME="Developer ID Application: Chih-Wei Su (BNH8YS88T9)"
PROFILE=awpr-notary
APPLE_ID=awaysu@gmail.com
TEAM_ID=BNH8YS88T9

echo "==> 1/6  Keychain password file ($PASSFILE)"
mkdir -p "$(dirname "$PASSFILE")"
chmod 700 "$(dirname "$PASSFILE")"
if [ -f "$PASSFILE" ]; then
    PW=$(cat "$PASSFILE")
    echo "    reusing existing password file"
else
    PW=$(openssl rand -hex 24)
    (umask 077; printf '%s' "$PW" > "$PASSFILE")
    echo "    created"
fi

echo "==> 2/6  Keychain ($KC)"
if [ ! -f "$KC" ]; then
    security create-keychain -p "$PW" "$KC"
    echo "    created"
fi
security unlock-keychain -p "$PW" "$KC"
# No -l / -u: never lock on sleep, never lock after a timeout.
security set-keychain-settings "$KC"

echo "==> 3/6  Copying the identity from the login keychain"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
P12="$TMP/identity.p12"
TP=$(openssl rand -hex 16)
# Exports every identity in the login keychain (there is also a localhost test cert);
# the extra one is harmless and the scripts pin the identity by name.
security export -k "$LOGIN" -t identities -f pkcs12 -P "$TP" -o "$P12"
# "The specified item already exists" on a re-run is fine.
security import "$P12" -k "$KC" -P "$TP" \
    -T /usr/bin/codesign -T /usr/bin/security -T /usr/bin/productbuild -T /usr/bin/productsign \
    || true
rm -f "$P12"
# Lets codesign (an "apple-tool") use the key without a prompt — the part a GUI
# "allow all applications" click does not cover.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PW" "$KC" > /dev/null

echo "==> 4/6  Putting it FIRST in the keychain search list"
# First, not last: codesign resolves the private key through the search list in order
# and ignores --keychain for that step, so with the login keychain ahead of ours it hits
# the locked login copy of the same key and fails with errSecInternalComponent
# (2026-09-02, measured: last → fails, first → signs). Lookups that miss here fall
# through to the login keychain, and new items still go to the *default* keychain
# (login), so putting ours first changes nothing for GUI apps.
existing=()
while IFS= read -r line; do
    line=${line#"${line%%[!$' \t']*}"}     # trim leading blanks
    line=${line%\"}; line=${line#\"}
    [ -n "$line" ] && [ "$line" != "$KC" ] && existing+=("$line")
done < <(security list-keychains -d user)
security list-keychains -d user -s "$KC" ${existing[@]+"${existing[@]}"}

echo "==> 5/6  notarytool profile '$PROFILE' in the signing keychain"
read -r -s -p "    Apple app-specific password for $APPLE_ID: " AP; echo
xcrun notarytool store-credentials "$PROFILE" --keychain "$KC" \
    --apple-id "$APPLE_ID" --team-id "$TEAM_ID" --password "$AP"
unset AP

echo "==> 6/6  Verifying"
security find-identity -v -p codesigning "$KC" | grep -F "$IDENTITY_NAME" \
    || { echo "!! identity not found in $KC" >&2; exit 1; }
echo
echo "Done. From now on Scripts/build_app.sh and Scripts/sign_and_notarize.sh unlock"
echo "$KC themselves, so they work over SSH too."
