#!/usr/bin/env bash
# Build, sign, notarize and staple the macOS DMG.
#
# One-time setup:
#   - "Developer ID Application" certificate in the login keychain
#   - xcrun notarytool store-credentials ofm-notary
#
# Env overrides:
#   APPLE_SIGNING_IDENTITY  signing identity (default: first Developer ID Application cert)
#   NOTARY_PROFILE          notarytool keychain profile (default: ofm-notary)
#
# Extra args go to `tauri build`, e.g. scripts/release.sh --target universal-apple-darwin
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
  APPLE_SIGNING_IDENTITY=$(security find-identity -v -p codesigning \
    | sed -n 's/.*"\(Developer ID Application: [^"]*\)".*/\1/p' | head -n1)
fi
if [[ -z "$APPLE_SIGNING_IDENTITY" ]]; then
  echo "error: no Developer ID Application certificate found" >&2
  exit 1
fi
export APPLE_SIGNING_IDENTITY
NOTARY_PROFILE="${NOTARY_PROFILE:-ofm-notary}"

marker=$(mktemp)
trap 'rm -f "$marker"' EXIT

npx tauri build "$@"

dmg=$(find src-tauri/target -path '*/bundle/dmg/*.dmg' -newer "$marker" | head -n1)
if [[ -z "$dmg" ]]; then
  echo "error: no freshly built DMG found" >&2
  exit 1
fi

echo "Notarizing $dmg"
xcrun notarytool submit "$dmg" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$dmg"
spctl -a -vv -t open --context context:primary-signature "$dmg"

echo "Done: $dmg"
