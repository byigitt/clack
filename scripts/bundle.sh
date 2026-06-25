#!/usr/bin/env bash
# Build clack.app — a macOS bundle is just a folder + Info.plist + binary + an
# ad-hoc code signature (so the Accessibility grant survives rebuilds).
set -euo pipefail
cd "$(dirname "$0")/.."

APP="dist/clack.app"
echo "==> cargo build --release"
cargo build --release

echo "==> assembling $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp Info.plist "$APP/Contents/Info.plist"
cp target/release/clack "$APP/Contents/MacOS/clack"
[ -f assets/clack.icns ] && cp assets/clack.icns "$APP/Contents/Resources/clack.icns" || true

echo "==> ad-hoc codesign (stable identity for Accessibility)"
codesign --force --deep --sign - "$APP"

echo "==> done: $APP"
codesign -dv "$APP" 2>&1 | grep -E "Identifier|Signature" || true
