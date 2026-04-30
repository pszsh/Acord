#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Darwin) ;;
    *) echo "wrong platform: $(uname -s) — use cargo xtask build-universal" >&2; exit 1;;
esac

BUILD="$ROOT/build"
APP="$BUILD/bin/Acord.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
SDK=$(xcrun --show-sdk-path)
export MACOSX_DEPLOYMENT_TARGET=14.0

echo "Building Rust workspace (Universal)..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin

cargo build --release -p acord-viewport --target aarch64-apple-darwin
cargo build --release -p acord-viewport --target x86_64-apple-darwin

mkdir -p "$ROOT/target/universal"
lipo -create \
  "$ROOT/target/aarch64-apple-darwin/release/libacord_viewport.a" \
  "$ROOT/target/x86_64-apple-darwin/release/libacord_viewport.a" \
  -output "$ROOT/target/universal/libacord_viewport.a"

# TODO: regenerate AppIcon.icns from assets/Acord.svg here (see build.sh).

mkdir -p "$MACOS" "$RESOURCES"
cp "$ROOT/macos/Info.plist" "$CONTENTS/Info.plist"
[ -f "$BUILD/AppIcon.icns" ] && cp "$BUILD/AppIcon.icns" "$RESOURCES/AppIcon.icns"

echo "Compiling Swift (Universal)..."
SWIFT_FILES=("$ROOT"/macos/src/*.swift)
RUST_INCLUDES=(-import-objc-header "$ROOT/viewport/include/acord.h" -L "$ROOT/target/universal" -lacord_viewport)

swiftc -target arm64-apple-macosx14.0 -sdk "$SDK" "${RUST_INCLUDES[@]}" \
  -framework Cocoa -framework SwiftUI -framework Metal -framework MetalKit -O \
  -o "$MACOS/Acord_arm64" "${SWIFT_FILES[@]}"

swiftc -target x86_64-apple-macosx14.0 -sdk "$SDK" "${RUST_INCLUDES[@]}" \
  -framework Cocoa -framework SwiftUI -framework Metal -framework MetalKit -O \
  -o "$MACOS/Acord_x86" "${SWIFT_FILES[@]}"

lipo -create "$MACOS/Acord_arm64" "$MACOS/Acord_x86" -output "$MACOS/Acord"
rm "$MACOS/Acord_arm64" "$MACOS/Acord_x86"

codesign --force --sign - --deep "$APP"

echo "Built Universal App: $APP"
