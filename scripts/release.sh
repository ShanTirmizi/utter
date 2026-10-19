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
# In-app updates: the app is also packed as Utter.app.tar.gz, signed with the
# update key (~/.tauri/utter-updater.key, or UTTER_UPDATER_KEY; its password is
# in the login keychain as "utter-updater-key"), and described in latest.json.
# Utter's Install and restart reads latest.json from the newest GitHub release
# and accepts the package only if its signature matches the public key in
# tauri.conf.json and names the version announced. Lose the key and copies
# out there can never update themselves again, so keep a backup of it.
#
# Writes dist-release/Utter-apple-silicon.dmg, Utter.app.tar.gz(.sig) and
# latest.json, and prints the DMG's SHA-256. Upload all four to the release.
set -euo pipefail
cd "$(dirname "$0")/.."

identity="${UTTER_SIGNING_IDENTITY:-}"
profile="${UTTER_NOTARY_PROFILE:-}"
out="dist-release"
app="src-tauri/target/release/bundle/macos/Utter.app"
dmg="$out/Utter-apple-silicon.dmg"
update="$out/Utter.app.tar.gz"
update_key="${UTTER_UPDATER_KEY:-$HOME/.tauri/utter-updater.key}"
version=$(/usr/bin/python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json"))["version"])')
repo="https://github.com/ShanTirmizi/utter"

# A signed release without the update package would strand everyone on it, so
# check for the key before spending minutes on the build.
if [ -n "$identity" ] && { [ ! -f "$update_key" ] || ! security find-generic-password -s utter-updater-key >/dev/null 2>&1; }; then
  echo "The update key ($update_key) or its keychain password is missing." >&2
  exit 1
fi

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

# The update is the app as notarised and stapled, so it opens without a
# Gatekeeper prompt once it is swapped in.
if [ -f "$update_key" ]; then
  echo "== Update package"
  mkdir -p "$out"
  rm -f "$update" "$update.sig" "$out/latest.json"
  COPYFILE_DISABLE=1 tar -C "$(dirname "$app")" -czf "$update" Utter.app
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(security find-generic-password -s utter-updater-key -w)" \
    pnpm -s tauri signer sign -f "$update_key" --app-version "$version" "$update" >/dev/null
  UTTER_VERSION="$version" UTTER_URL="$repo/releases/download/v$version/Utter.app.tar.gz" \
    UTTER_SIG="$update.sig" /usr/bin/python3 - "$out/latest.json" <<'PY'
import datetime, json, os, sys
entry = {"signature": open(os.environ["UTTER_SIG"]).read().strip(), "url": os.environ["UTTER_URL"]}
manifest = {
    "version": os.environ["UTTER_VERSION"],
    "notes": "See what's new on GitHub.",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    # The updater looks for the bundle-specific key first, then the plain one.
    "platforms": {"darwin-aarch64-app": entry, "darwin-aarch64": entry},
}
json.dump(manifest, open(sys.argv[1], "w"), indent=2)
PY
else
  echo "== No update key, so no update package"
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
ls -la "$out"
shasum -a 256 "$dmg"
