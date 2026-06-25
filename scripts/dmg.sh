#!/usr/bin/env bash
# Build a drag-to-install DMG: clack.app + an Applications shortcut.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:-0.1.0}"
APP="dist/clack.app"
DMG="dist/clack-${VERSION}.dmg"
STAGE="$(mktemp -d)/clack"

[ -d "$APP" ] || { echo "build $APP first (./scripts/bundle.sh)"; exit 1; }

echo "==> staging"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

echo "==> creating $DMG"
rm -f "$DMG"
hdiutil create -volname "clack ${VERSION}" \
  -srcfolder "$STAGE" \
  -fs HFS+ -format UDZO -ov \
  "$DMG" >/dev/null

rm -rf "$STAGE"
echo "==> done: $DMG"
hdiutil imageinfo "$DMG" 2>/dev/null | grep -E "Format:|Compressed" | head -2 || true
