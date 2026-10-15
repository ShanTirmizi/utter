#!/bin/bash
# Build, sign, package and (when possible) notarise a release DMG.
#
#   UTTER_SIGNING_IDENTITY="Developer ID Application: Name (TEAMID)" \
#   UTTER_NOTARY_PROFILE=utter-notary \
#   scripts/release.sh
#
# The identity must be in your login keychain. The notary profile is one you
# saved yourself with `xcrun notarytool store-credentials`; the password never
# passes through this script. Without a profile the DMG is signed but not
# notarised, and macOS will ask users to approve the first launch by hand.
# Without an identity the app is signed ad hoc, as for a local build.
#
# Writes dist-release/Utter-apple-silicon.dmg and prints its SHA-256.
set -euo pipefail
cd "$(dirname "$0")/.."

identity="${UTTER_SIGNING_IDENTITY:-}"
profile="${UTTER_NOTARY_PROFILE:-}"
out="dist-release"
app="src-tauri/target/release/bundle/macos/Utter.app"
dmg="$out/Utter-apple-silicon.dmg"

echo "== Building"
if [ -n "$identity" ]; then
  APPLE_SIGNING_IDENTITY="$identity" pnpm tauri build
else
  pnpm tauri build
fi
codesign --verify --deep --strict "$app"

notarise() {
  # Submit, wait, and fail loudly with Apple's log if it is rejected.
  local file="$1" result id
  result=$(xcrun notarytool submit "$file" --keychain-profile "$profile" --wait --output-format json)
  id=$(printf '%s' "$result" | /usr/bin/python3 -c 'import json,sys; print(json.load(sys.stdin)["id"])')
  if ! printf '%s' "$result" | grep -q '"status" *: *"Accepted"'; then
    xcrun notarytool log "$id" --keychain-profile "$profile" || true
    echo "Notarisation was not accepted." >&2
    exit 1
  fi
}

if [ -n "$identity" ] && [ -n "$profile" ]; then
  echo "== Notarising the app"
  rm -f "$out/Utter.zip"; mkdir -p "$out"
  ditto -c -k --keepParent "$app" "$out/Utter.zip"
  notarise "$out/Utter.zip"
  xcrun stapler staple "$app"
  rm -f "$out/Utter.zip"
fi

echo "== Packaging"
stage=$(mktemp -d)
ditto "$app" "$stage/Utter.app"
ln -s /Applications "$stage/Applications"
mkdir -p "$out"
rm -f "$dmg"
hdiutil create -volname Utter -srcfolder "$stage" -ov -format UDZO -imagekey zlib-level=9 "$dmg" >/dev/null
rm -rf "$stage"
if [ -n "$identity" ]; then
  codesign --sign "$identity" --timestamp "$dmg"
fi

if [ -n "$identity" ] && [ -n "$profile" ]; then
  echo "== Notarising the DMG"
  notarise "$dmg"
  xcrun stapler staple "$dmg"
  spctl -a -vv -t open --context context:primary-signature "$dmg"
fi

echo "== Done"
ls -la "$dmg"
shasum -a 256 "$dmg"
