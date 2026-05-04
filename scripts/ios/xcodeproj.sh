#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if ! command -v xcodegen >/dev/null 2>&1; then
    echo "xcodegen not found. install with:" >&2
    echo "  brew install xcodegen" >&2
    exit 1
fi

# the user has esp-clang on PATH; make sure cc-rs picks apple's.
export CC=/usr/bin/clang
export CXX=/usr/bin/clang++
export IPHONEOS_DEPLOYMENT_TARGET=17.0

# build the staticlibs xcode will link against — both arches so xcode can target either.
echo "Building Rust staticlibs for both iOS targets (release)..."
cargo build --release --target aarch64-apple-ios -p acord-viewport
cargo build --release --target aarch64-apple-ios-sim -p acord-viewport

cd "$ROOT/ios"
echo "Generating Acord.xcodeproj..."
xcodegen generate

echo
echo "Generated: $ROOT/ios/Acord.xcodeproj"
echo "Open with: open $ROOT/ios/Acord.xcodeproj"
echo
echo "Build/run from xcode: pick a destination (your iPad or a sim) and hit ⌘R."
echo "If you change Rust code, re-run this command to rebuild the staticlibs."
