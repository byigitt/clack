#!/usr/bin/env bash
# Build a styled drag-to-install DMG: background art, positioned icons,
# clack.app + an Applications shortcut.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:-0.1.0}"
APP="dist/clack.app"
DMG="dist/clack-${VERSION}.dmg"
VOL="clack ${VERSION}"
TMPDMG="dist/.clack-rw.dmg"

[ -d "$APP" ] || { echo "build $APP first (./scripts/bundle.sh)"; exit 1; }

WORK="$(mktemp -d)/src"
mkdir -p "$WORK/.background"
cp -R "$APP" "$WORK/"
ln -s /Applications "$WORK/Applications"
# 1200x840 (=600x420 @2x) background — Finder renders it at 600x420 points.
cp assets/dmg-bg.png "$WORK/.background/bg.png"

echo "==> creating writable DMG"
rm -f "$TMPDMG" "$DMG"
hdiutil create -volname "$VOL" -srcfolder "$WORK" -fs HFS+ \
  -format UDRW -ov "$TMPDMG" >/dev/null
rm -rf "$WORK"

echo "==> mounting + styling"
MOUNT="$(hdiutil attach "$TMPDMG" -readwrite -noverify -noautoopen | grep "/Volumes/" | sed -E 's/.*(\/Volumes\/.*)/\1/')"
sleep 1

osascript <<APPLESCRIPT
tell application "Finder"
  tell disk "$VOL"
    open
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {200, 120, 800, 568}
    set vop to the icon view options of container window
    set arrangement of vop to not arranged
    set icon size of vop to 104
    set text size of vop to 12
    set background picture of vop to file ".background:bg.png"
    set position of item "clack.app" of container window to {158, 250}
    set position of item "Applications" of container window to {442, 250}
    update without registering applications
    delay 1
    close
  end tell
end tell
APPLESCRIPT

# Custom volume icon (best-effort; needs SetFile from CLT).
cp assets/clack.icns "$MOUNT/.VolumeIcon.icns" 2>/dev/null || true
SetFile -a C "$MOUNT" 2>/dev/null || true

sync
hdiutil detach "$MOUNT" -quiet || hdiutil detach "$MOUNT" -force -quiet

echo "==> compressing"
hdiutil convert "$TMPDMG" -format UDZO -imagekey zlib-level=9 -ov -o "$DMG" >/dev/null
rm -f "$TMPDMG"
echo "==> done: $DMG ($(du -h "$DMG" | cut -f1))"
