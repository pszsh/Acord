#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Darwin) ;;
    *) echo "wrong platform: $(uname -s) — iOS build requires macOS" >&2; exit 1;;
esac

# Default to simulator; override with `bash scripts/ios/build.sh device`.
TARGET="${1:-sim}"

case "$TARGET" in
    sim)
        RUST_TARGET="aarch64-apple-ios-sim"
        SDK_NAME="iphonesimulator"
        SWIFT_TARGET="arm64-apple-ios17.0-simulator"
        ;;
    device)
        RUST_TARGET="aarch64-apple-ios"
        SDK_NAME="iphoneos"
        SWIFT_TARGET="arm64-apple-ios17.0"
        ;;
    *)
        echo "usage: $0 [sim|device]" >&2
        exit 2
        ;;
esac

BUILD="$ROOT/build"
APP="$BUILD/ios/Acord.app"
RUST_LIB="$ROOT/target/$RUST_TARGET/release"

SDK="$(xcrun --sdk "$SDK_NAME" --show-sdk-path)"

# the user has esp-clang on PATH; fall through to apple's so cc-rs picks the right one.
export CC=/usr/bin/clang
export CXX=/usr/bin/clang++
export IPHONEOS_DEPLOYMENT_TARGET=17.0

echo "Building Rust workspace for $RUST_TARGET (release)..."
cargo build --release --target "$RUST_TARGET" -p acord-viewport

if [ ! -f "$RUST_LIB/libacord_viewport.a" ]; then
    echo "ERROR: libacord_viewport.a not found at $RUST_LIB" >&2
    exit 1
fi

# build app bundle (iOS bundles are flat — Info.plist, executable and resources live at the root)
rm -rf "$APP"
mkdir -p "$APP"
cp "$ROOT/ios/Info.plist" "$APP/Info.plist"

# generate icon (PNG variants required by iOS) — single 1024 master, scaled to bundle entries.
SVG="$ROOT/assets/Acord.svg"
if [ -f "$SVG" ] && command -v rsvg-convert >/dev/null 2>&1; then
    echo "Generating app icons..."
    for size in 20 29 40 58 60 76 80 87 120 152 167 180 1024; do
        rsvg-convert --width="$size" --height="$size" "$SVG" -o "$APP/AppIcon-${size}.png"
    done
fi

RUST_FLAGS=(-import-objc-header "$ROOT/viewport/include/acord.h" -L "$RUST_LIB" -lacord_viewport)

echo "Compiling Swift (release)..."
xcrun -sdk "$SDK_NAME" swiftc \
    -target "$SWIFT_TARGET" \
    -sdk "$SDK" \
    "${RUST_FLAGS[@]}" \
    -framework UIKit \
    -framework SwiftUI \
    -framework QuartzCore \
    -framework Metal \
    -framework MetalKit \
    -framework CoreGraphics \
    -framework CoreFoundation \
    -O \
    -o "$APP/Acord" \
    "$ROOT"/ios/src/*.swift

if [ "$TARGET" = "sim" ]; then
    codesign --force --sign - "$APP"
else
    # device build: embed provisioning profile + sign with a real identity.
    PROFILE="${ACORD_IOS_PROFILE:-$HOME/Downloads/All.mobileprovision}"
    if [ ! -f "$PROFILE" ]; then
        echo "ERROR: provisioning profile not found at $PROFILE" >&2
        echo "       set ACORD_IOS_PROFILE to point at a valid .mobileprovision" >&2
        exit 1
    fi
    cp "$PROFILE" "$APP/embedded.mobileprovision"

    ENT="$BUILD/ios/entitlements.plist"
    security cms -D -i "$PROFILE" 2>/dev/null \
        | plutil -extract Entitlements xml1 -o "$ENT" - \
        || { echo "ERROR: could not extract entitlements from profile" >&2; exit 1; }

    # find a codesigning identity that's in the profile's DeveloperCertificates list.
    # we pull each cert's SHA1 out of the profile, then pick whichever one find-identity
    # also lists as valid in the keychain. fail loudly if none match.
    TMPDIR_PROF="$(mktemp -d)"
    PROFILE_PLIST="$TMPDIR_PROF/profile.plist"
    security cms -D -i "$PROFILE" > "$PROFILE_PLIST" 2>/dev/null

    PROFILE_SHAS=""
    for i in 0 1 2 3 4 5 6 7 8 9; do
        if ! plutil -extract "DeveloperCertificates.$i" raw -o "$TMPDIR_PROF/c$i.b64" "$PROFILE_PLIST" >/dev/null 2>&1; then
            break
        fi
        base64 -D -i "$TMPDIR_PROF/c$i.b64" -o "$TMPDIR_PROF/c$i.cer"
        sha=$(openssl x509 -inform der -in "$TMPDIR_PROF/c$i.cer" -fingerprint -noout 2>/dev/null \
              | sed 's/.*=//;s/://g')
        PROFILE_SHAS="$PROFILE_SHAS $sha"
    done

    KEYCHAIN_SHAS=$(security find-identity -v 2>/dev/null \
                    | awk '/[0-9A-F]{40}/ {gsub(/[^0-9A-F]/, "", $2); print $2}')

    IDENTITY=""
    for s in $PROFILE_SHAS; do
        if echo "$KEYCHAIN_SHAS" | grep -qi "^$s$"; then
            IDENTITY="$s"
            break
        fi
    done
    rm -rf "$TMPDIR_PROF"

    if [ -z "$IDENTITY" ]; then
        echo "ERROR: no codesigning identity in your keychain matches any cert in the profile" >&2
        echo "       profile certs:$PROFILE_SHAS" >&2
        exit 1
    fi

    echo "Signing with $IDENTITY..."
    codesign --force \
        --sign "$IDENTITY" \
        --entitlements "$ENT" \
        --options runtime \
        --timestamp=none \
        "$APP"
fi

echo "Built: $APP"
