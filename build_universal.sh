#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
BUILD="$ROOT/build"
APP="$BUILD/bin/Acord.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
SDK=$(xcrun --show-sdk-path)
export MACOSX_DEPLOYMENT_TARGET=14.0

# 1. Build Rust for both architectures
echo "Building Rust workspace (Universal)..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin

cargo build --release -p acord-viewport --target aarch64-apple-darwin
cargo build --release -p acord-viewport --target x86_64-apple-darwin

# 2. Create Universal Rust Static Lib
mkdir -p "$ROOT/target/universal"
lipo -create \
  "$ROOT/target/aarch64-apple-darwin/release/libacord_viewport.a" \
  "$ROOT/target/x86_64-apple-darwin/release/libacord_viewport.a" \
  -output "$ROOT/target/universal/libacord_viewport.a"

# --- Icon Generation (Remains the same) ---
# [Your existing rsvg-convert and iconutil code here]

# --- Bundle structure ---
mkdir -p "$MACOS" "$RESOURCES"
cp "$ROOT/Info.plist" "$CONTENTS/Info.plist"
[ -f "$BUILD/AppIcon.icns" ] && cp "$BUILD/AppIcon.icns" "$RESOURCES/AppIcon.icns"

# 3. Compile Swift for both architectures
echo "Compiling Swift (Universal)..."
SWIFT_FILES=("$ROOT"/src/*.swift)
RUST_INCLUDES=(-import-objc-header "$ROOT/viewport/include/acord.h" -L "$ROOT/target/universal" -lacord_viewport)

# Compile arm64 slice
swiftc -target arm64-apple-macosx14.0 -sdk "$SDK" "${RUST_INCLUDES[@]}" \
  -framework Cocoa -framework SwiftUI -framework Metal -framework MetalKit -O \
  -o "$MACOS/Acord_arm64" "${SWIFT_FILES[@]}"

# Compile x86_64 slice
swiftc -target x86_64-apple-macosx14.0 -sdk "$SDK" "${RUST_INCLUDES[@]}" \
  -framework Cocoa -framework SwiftUI -framework Metal -framework MetalKit -O \
  -o "$MACOS/Acord_x86" "${SWIFT_FILES[@]}"

# 4. Merge Swift binaries into one Universal binary
lipo -create "$MACOS/Acord_arm64" "$MACOS/Acord_x86" -output "$MACOS/Acord"
rm "$MACOS/Acord_arm64" "$MACOS/Acord_x86"

# 5. Code sign the universal bundle
codesign --force --sign - --deep "$APP"

echo "Successfully built Universal App: $APP"

