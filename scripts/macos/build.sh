#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Darwin) ;;
    *) echo "wrong platform: $(uname -s) — use cargo xtask build" >&2; exit 1;;
esac

BUILD="$ROOT/build"
APP="$BUILD/bin/Acord.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

SDK=$(xcrun --show-sdk-path)
RUST_LIB="$ROOT/target/release"

export MACOSX_DEPLOYMENT_TARGET=14.0
export ZERO_AR_DATE=0

echo "Building Rust workspace (release)..."
cargo build --release -p acord-viewport

if [ ! -f "$RUST_LIB/libacord_viewport.a" ]; then
    echo "ERROR: libacord_viewport.a not found at $RUST_LIB" >&2
    exit 1
fi

RUST_FLAGS=(-import-objc-header "$ROOT/viewport/include/acord.h" -L "$RUST_LIB" -lacord_viewport)

# App icon from SVG via rsvg-convert.
SVG="$ROOT/assets/Acord.svg"
if [ -f "$SVG" ]; then
    echo "Generating app icon..."
    ICONSET="$BUILD/AppIcon.iconset"
    mkdir -p "$ICONSET"
    for size in 16 32 64 128 256 512 1024; do
        rsvg-convert --width="$size" --height="$size" "$SVG" -o "$ICONSET/icon_${size}.png"
    done
    cp "$ICONSET/icon_16.png"   "$ICONSET/icon_16x16.png"
    cp "$ICONSET/icon_32.png"   "$ICONSET/icon_16x16@2x.png"
    cp "$ICONSET/icon_32.png"   "$ICONSET/icon_32x32.png"
    cp "$ICONSET/icon_64.png"   "$ICONSET/icon_32x32@2x.png"
    cp "$ICONSET/icon_128.png"  "$ICONSET/icon_128x128.png"
    cp "$ICONSET/icon_256.png"  "$ICONSET/icon_128x128@2x.png"
    cp "$ICONSET/icon_256.png"  "$ICONSET/icon_256x256.png"
    cp "$ICONSET/icon_512.png"  "$ICONSET/icon_256x256@2x.png"
    cp "$ICONSET/icon_512.png"  "$ICONSET/icon_512x512.png"
    cp "$ICONSET/icon_1024.png" "$ICONSET/icon_512x512@2x.png"
    rm -f "$ICONSET"/icon_16.png "$ICONSET"/icon_32.png "$ICONSET"/icon_64.png \
          "$ICONSET"/icon_128.png "$ICONSET"/icon_256.png "$ICONSET"/icon_512.png \
          "$ICONSET"/icon_1024.png
    iconutil -c icns "$ICONSET" -o "$BUILD/AppIcon.icns"
    rm -rf "$ICONSET"
fi

mkdir -p "$MACOS" "$RESOURCES"
cp "$ROOT/Info.plist" "$CONTENTS/Info.plist"
[ -f "$BUILD/AppIcon.icns" ] && cp "$BUILD/AppIcon.icns" "$RESOURCES/AppIcon.icns"

echo "Compiling Swift (release)..."
swiftc \
    -target arm64-apple-macosx14.0 \
    -sdk "$SDK" \
    "${RUST_FLAGS[@]}" \
    -framework Cocoa \
    -framework SwiftUI \
    -framework Metal \
    -framework MetalKit \
    -framework QuartzCore \
    -framework CoreGraphics \
    -framework CoreFoundation \
    -O \
    -o "$MACOS/Acord" \
    "$ROOT"/src/*.swift

codesign --force --sign - "$APP"

echo "Built: $APP"
open "$APP"
