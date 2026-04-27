#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
DEST="/Applications/Acord.app"

echo "Building release..."
"$ROOT/build.sh"

# Kill running instance before replacing
pkill -f "Acord.app/Contents/MacOS/Acord" 2>/dev/null || true
sleep 0.5

echo "Installing to $DEST..."
rm -rf "$DEST"
cp -R "$ROOT/build/bin/Acord.app" "$DEST"

echo "Installed: $DEST"
