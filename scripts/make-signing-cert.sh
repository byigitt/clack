#!/usr/bin/env bash
# Create a stable self-signed code-signing identity ("clack-dev") in the login
# keychain. Signing the app with a fixed identity (instead of ad-hoc) keeps the
# macOS Accessibility grant alive across rebuilds. Run once.
set -euo pipefail

NAME="${1:-clack-dev}"
KC="$HOME/Library/Keychains/login.keychain-db"

if security find-certificate -c "$NAME" >/dev/null 2>&1; then
  echo "identity '$NAME' already exists"
  exit 0
fi

TMP="$(mktemp -d)"
openssl req -x509 -newkey rsa:2048 -keyout "$TMP/key.pem" -out "$TMP/cert.pem" \
  -days 3650 -nodes -subj "/CN=$NAME" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" >/dev/null 2>&1

security import "$TMP/key.pem"  -k "$KC" -T /usr/bin/codesign -A >/dev/null
security import "$TMP/cert.pem" -k "$KC" -T /usr/bin/codesign -A >/dev/null
# Let codesign use the key without a keychain prompt.
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "" "$KC" >/dev/null 2>&1 || true
rm -rf "$TMP"

echo "created code-signing identity '$NAME'"
