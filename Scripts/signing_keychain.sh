#!/bin/bash
# Sourced by build_app.sh and sign_and_notarize.sh — NOT run directly.
#
# Makes codesign / notarytool work from a headless session (SSH, CI, Claude Code over
# SSH). The login keychain starts *locked* in every non-GUI session and nothing can
# prompt to unlock it, so codesign fails with errSecInternalComponent no matter how the
# key's access control is set (2026-09-02). The fix is a dedicated keychain that holds
# only the Developer ID identity and the notarytool profile, is set to never auto-lock,
# and whose (random) password lives in a 0600 file so the scripts can unlock it
# themselves. Scripts/setup_signing_keychain.sh creates it — once, in Terminal.app.
#
# When the keychain is absent everything falls back to the default search list, i.e.
# exactly the pre-2026-09-02 behaviour (works in a GUI session, fails over SSH).

AWPR_KEYCHAIN="$HOME/Library/Keychains/awpr-signing.keychain-db"
AWPR_KEYCHAIN_PASS_FILE="$HOME/.config/awpr/signing-keychain.pass"
AWPR_KEYCHAIN_ARGS=()

if [ -f "$AWPR_KEYCHAIN" ] && [ -f "$AWPR_KEYCHAIN_PASS_FILE" ]; then
    if security unlock-keychain -p "$(cat "$AWPR_KEYCHAIN_PASS_FILE")" "$AWPR_KEYCHAIN"; then
        AWPR_KEYCHAIN_ARGS=(--keychain "$AWPR_KEYCHAIN")
        # codesign finds the private key through the search list *in order* and ignores
        # --keychain for that step; if the login keychain comes first its locked copy of
        # the key wins and signing fails. Keep ours first (idempotent).
        if [ "$(security list-keychains -d user | head -1 | tr -d ' "')" != "$AWPR_KEYCHAIN" ]; then
            _awpr_rest=()
            while IFS= read -r _l; do
                _l=${_l#"${_l%%[!$' \t']*}"}; _l=${_l%\"}; _l=${_l#\"}
                [ -n "$_l" ] && [ "$_l" != "$AWPR_KEYCHAIN" ] && _awpr_rest+=("$_l")
            done < <(security list-keychains -d user)
            security list-keychains -d user -s "$AWPR_KEYCHAIN" ${_awpr_rest[@]+"${_awpr_rest[@]}"}
        fi
    else
        echo "!! could not unlock $AWPR_KEYCHAIN — falling back to the default keychains" >&2
    fi
fi

# codesign / notarytool with the signing keychain pinned when it exists.
# (The ${arr[@]+...} form keeps `set -u` happy on macOS's bash 3.2 when the array is empty.)
cs() { codesign ${AWPR_KEYCHAIN_ARGS[@]+"${AWPR_KEYCHAIN_ARGS[@]}"} "$@"; }
nt() { xcrun notarytool "$@" ${AWPR_KEYCHAIN_ARGS[@]+"${AWPR_KEYCHAIN_ARGS[@]}"}; }
