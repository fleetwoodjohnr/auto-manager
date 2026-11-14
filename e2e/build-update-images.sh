#!/usr/bin/env bash
# Build two signed AppImages (OLD and NEW versions) that update from a local test server, for
# e2e/update-appimage.mjs. Uses a throwaway signing key, never the release key.
#   e2e/build-update-images.sh <out-dir> [old-version] [new-version]
set -euo pipefail
out=${1:?out dir}; old=${2:-0.1.0}; new=${3:-0.1.1}
mkdir -p "$out"
key="$out/test.key"
[ -f "$key" ] || npx tauri signer generate --ci -p "" -w "$key" >/dev/null
pub=$(cat "$key.pub")
for v in "$old" "$new"; do
  cat > "$out/conf-$v.json" <<JSON
{ "version": "$v", "bundle": { "createUpdaterArtifacts": true },
  "plugins": { "updater": { "pubkey": "$pub", "endpoints": ["http://127.0.0.1:8765/latest.json"], "dangerousInsecureTransportProtocol": true } } }
JSON
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    npx tauri build --debug --bundles appimage --config "$out/conf-$v.json"
  img=$(ls target/debug/bundle/appimage/*.AppImage)
  cp "$img" "$out/auto-manager-$v.AppImage"
  cp "$img.sig" "$out/auto-manager-$v.AppImage.sig"
  rm -f "$img" "$img.sig"
done
echo "Built $out/auto-manager-$old.AppImage and $out/auto-manager-$new.AppImage"
