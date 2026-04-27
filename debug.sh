#!/usr/bin/env bash
set -euo pipefail

# Debug build — same wiring as build.sh but unoptimised, with -g, and
# launched in the foreground so Rust panics print straight to this terminal
# (the panic hook in viewport/src/lib.rs flushes stderr before SIGABRT).

ROOT="$(cd "$(dirname "$0")" && pwd)"
BUILD="$ROOT/build"
APP="$BUILD/bin/Acord.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"

SDK=$(xcrun --show-sdk-path)
RUST_LIB="$ROOT/target/debug"

export MACOSX_DEPLOYMENT_TARGET=14.0
export ZERO_AR_DATE=0
export RUST_BACKTRACE=1

echo "Building Rust workspace (debug)..."
cd "$ROOT" && cargo build -p acord-viewport

if [ ! -f "$RUST_LIB/libacord_viewport.a" ]; then
    echo "ERROR: libacord_viewport.a not found at $RUST_LIB"
    exit 1
fi

RUST_FLAGS=(-import-objc-header "$ROOT/viewport/include/acord.h" -L "$RUST_LIB" -lacord_viewport)

# --- Bundle structure ---
mkdir -p "$MACOS" "$RESOURCES"
cp "$ROOT/Info.plist" "$CONTENTS/Info.plist"
if [ -f "$BUILD/AppIcon.icns" ]; then
    cp "$BUILD/AppIcon.icns" "$RESOURCES/AppIcon.icns"
fi

# --- Compile Swift (debug) ---
echo "Compiling Swift (debug)..."
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
    -Onone -g \
    -o "$MACOS/Acord" \
    "$ROOT"/src/*.swift

codesign --force --sign - "$APP"

# --- Kill existing, launch in foreground so stderr lands here ---
pkill -f "Acord.app/Contents/MacOS/Acord" 2>/dev/null || true
sleep 0.3

echo
echo "Launching $MACOS/Acord — Rust panics will print below."
echo "(Ctrl+C to exit, or quit Acord normally.)"
echo "----------------------------------------------------------"
exec "$MACOS/Acord"
