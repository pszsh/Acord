#!/usr/bin/env bash
set -euo pipefail

# Cross-compile + zip distributables from a single macOS host.
#
# Six targets:
#   macos-aarch64    macos-x86_64
#   windows-aarch64  windows-x86_64
#   linux-aarch64    linux-x86_64
#
# Output: dist/acord-<target>.zip per target.
#
# Tooling:
#   - rustup, swiftc, zip, codesign — assumed present on a dev mac
#   - rsvg-convert — brew install librsvg (for the macOS app icon)
#   - zig + cargo-zigbuild — used for windows/linux cross-compile
#       brew install zig
#       cargo install cargo-zigbuild

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Darwin) ;;
    *) echo "package.sh: macOS host only (need swiftc + codesign)" >&2; exit 1;;
esac

ALL_TARGETS=(
    macos-aarch64
    macos-x86_64
    windows-aarch64
    windows-x86_64
    linux-aarch64
    linux-x86_64
)

usage() {
    cat >&2 <<EOF
usage: cargo xtask package --all
       cargo xtask package --target <name> [--target <name> ...]

targets: ${ALL_TARGETS[*]}
EOF
    exit 2
}

TARGETS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --all) TARGETS=("${ALL_TARGETS[@]}"); shift ;;
        --target) [ $# -ge 2 ] || usage; TARGETS+=("$2"); shift 2 ;;
        -h|--help) usage ;;
        *) echo "unknown arg: $1" >&2; usage ;;
    esac
done
[ ${#TARGETS[@]} -eq 0 ] && usage

need() { command -v "$1" >/dev/null 2>&1 || { echo "ERROR: $1 not found. $2" >&2; exit 1; }; }

need rustup "install rustup from https://rustup.rs"
need swiftc "install Xcode Command Line Tools (xcode-select --install)"
need zip "comes with macOS"

NEEDS_ZIG=0
for t in "${TARGETS[@]}"; do
    case "$t" in windows-*|linux-*) NEEDS_ZIG=1 ;; esac
done
if [ $NEEDS_ZIG -eq 1 ]; then
    need zig "brew install zig"
    need cargo-zigbuild "cargo install cargo-zigbuild"
fi

PKG="$ROOT/build/package"
DIST="$ROOT/dist"
mkdir -p "$PKG" "$DIST"

# Generate a 256px PNG once, reused by Windows + Linux. macOS uses a separate
# .icns set generated below.
ICON_PNG="$ROOT/build/icon.png"
if [ ! -f "$ICON_PNG" ] || [ "$ROOT/assets/Acord.svg" -nt "$ICON_PNG" ]; then
    if command -v rsvg-convert >/dev/null 2>&1 && [ -f "$ROOT/assets/Acord.svg" ]; then
        rsvg-convert --width 256 --height 256 "$ROOT/assets/Acord.svg" -o "$ICON_PNG"
    fi
fi

ensure_icns() {
    local icns="$ROOT/build/AppIcon.icns"
    if [ -f "$icns" ] && [ "$ROOT/assets/Acord.svg" -ot "$icns" ]; then return; fi
    [ -f "$ROOT/assets/Acord.svg" ] || return 0
    command -v rsvg-convert >/dev/null 2>&1 || return 0
    command -v iconutil >/dev/null 2>&1 || return 0

    local iconset="$ROOT/build/AppIcon.iconset"
    rm -rf "$iconset"
    mkdir -p "$iconset"
    for size in 16 32 64 128 256 512 1024; do
        rsvg-convert --width="$size" --height="$size" \
            "$ROOT/assets/Acord.svg" -o "$iconset/icon_${size}.png"
    done
    cp "$iconset/icon_16.png"   "$iconset/icon_16x16.png"
    cp "$iconset/icon_32.png"   "$iconset/icon_16x16@2x.png"
    cp "$iconset/icon_32.png"   "$iconset/icon_32x32.png"
    cp "$iconset/icon_64.png"   "$iconset/icon_32x32@2x.png"
    cp "$iconset/icon_128.png"  "$iconset/icon_128x128.png"
    cp "$iconset/icon_256.png"  "$iconset/icon_128x128@2x.png"
    cp "$iconset/icon_256.png"  "$iconset/icon_256x256.png"
    cp "$iconset/icon_512.png"  "$iconset/icon_256x256@2x.png"
    cp "$iconset/icon_512.png"  "$iconset/icon_512x512.png"
    cp "$iconset/icon_1024.png" "$iconset/icon_512x512@2x.png"
    rm -f "$iconset"/icon_16.png "$iconset"/icon_32.png "$iconset"/icon_64.png \
          "$iconset"/icon_128.png "$iconset"/icon_256.png "$iconset"/icon_512.png \
          "$iconset"/icon_1024.png
    iconutil -c icns "$iconset" -o "$icns"
    rm -rf "$iconset"
}

zip_target() {
    local target="$1" path="$2"
    local out="$DIST/acord-${target}.zip"
    rm -f "$out"
    (cd "$(dirname "$path")" && zip -r -q "$out" "$(basename "$path")")
    echo "  → $out  ($(du -h "$out" | cut -f1))"
}

build_macos() {
    local arch="$1" rust_target swift_target
    case "$arch" in
        aarch64) rust_target=aarch64-apple-darwin; swift_target=arm64-apple-macosx14.0 ;;
        x86_64)  rust_target=x86_64-apple-darwin;  swift_target=x86_64-apple-macosx14.0 ;;
    esac

    rustup target add "$rust_target" >/dev/null 2>&1 || true
    ensure_icns

    echo "==> macOS $arch  ($rust_target / $swift_target)"

    export MACOSX_DEPLOYMENT_TARGET=14.0
    export ZERO_AR_DATE=0
    cargo build --release -p acord-viewport --target "$rust_target"

    local rust_lib="$ROOT/target/$rust_target/release"
    [ -f "$rust_lib/libacord_viewport.a" ] \
        || { echo "ERROR: libacord_viewport.a missing for $rust_target" >&2; exit 1; }

    local stage="$PKG/macos-${arch}"
    local app="$stage/Acord.app"
    rm -rf "$stage"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$ROOT/Info.plist" "$app/Contents/Info.plist"
    [ -f "$ROOT/build/AppIcon.icns" ] && cp "$ROOT/build/AppIcon.icns" "$app/Contents/Resources/AppIcon.icns"

    local sdk
    sdk=$(xcrun --show-sdk-path)
    swiftc \
        -target "$swift_target" \
        -sdk "$sdk" \
        -import-objc-header "$ROOT/viewport/include/acord.h" \
        -L "$rust_lib" -lacord_viewport \
        -framework Cocoa -framework SwiftUI \
        -framework Metal -framework MetalKit \
        -framework QuartzCore -framework CoreGraphics -framework CoreFoundation \
        -O \
        -o "$app/Contents/MacOS/Acord" \
        "$ROOT"/src/*.swift

    codesign --force --sign - "$app"
    zip_target "macos-${arch}" "$app"
}

build_windows() {
    local arch="$1" rust_target
    case "$arch" in
        aarch64) rust_target=aarch64-pc-windows-msvc ;;
        x86_64)  rust_target=x86_64-pc-windows-msvc ;;
    esac

    rustup target add "$rust_target" >/dev/null 2>&1 || true

    echo "==> Windows $arch  ($rust_target via cargo-zigbuild)"

    cargo zigbuild --release -p acord-windows --target "$rust_target"

    local stage="$PKG/windows-${arch}/Acord"
    rm -rf "$stage"
    mkdir -p "$stage"

    cp "$ROOT/target/$rust_target/release/acord.exe" "$stage/Acord.exe"
    [ -f "$ICON_PNG" ] && cp "$ICON_PNG" "$stage/icon.png"
    [ -f "$ROOT/LICENCE" ] && cp "$ROOT/LICENCE" "$stage/LICENCE"
    [ -f "$ROOT/README.md" ] && cp "$ROOT/README.md" "$stage/README.md"

    zip_target "windows-${arch}" "$stage"
}

build_linux() {
    local arch="$1" rust_target
    case "$arch" in
        aarch64) rust_target=aarch64-unknown-linux-gnu ;;
        x86_64)  rust_target=x86_64-unknown-linux-gnu ;;
    esac

    rustup target add "$rust_target" >/dev/null 2>&1 || true

    echo "==> Linux $arch  (${rust_target}.2.17 via cargo-zigbuild — both x11+wayland linked)"

    # The .2.17 suffix targets glibc 2.17 (CentOS 7 baseline) for max distro
    # compatibility. zigbuild handles the symbol versioning via zig cc.
    cargo zigbuild --release -p acord-linux --target "${rust_target}.2.17"

    local stage="$PKG/linux-${arch}/acord"
    rm -rf "$stage"
    mkdir -p "$stage"

    cp "$ROOT/target/$rust_target/release/acord" "$stage/Acord"
    chmod +x "$stage/Acord"
    [ -f "$ICON_PNG" ] && cp "$ICON_PNG" "$stage/icon.png"
    [ -f "$ROOT/LICENCE" ] && cp "$ROOT/LICENCE" "$stage/LICENCE"
    [ -f "$ROOT/README.md" ] && cp "$ROOT/README.md" "$stage/README.md"

    # Self-contained installer the user runs after unzipping. Outer EOF is
    # single-quoted so $BIN_DIR / $HERE / etc. survive into the install
    # script verbatim and expand only when the user runs it.
    cat > "$stage/install.sh" <<'INSTALLER_EOF'
#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"
mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

pkill -x Acord 2>/dev/null || true
sleep 0.2

install -m 755 "$HERE/Acord" "$BIN_DIR/Acord"
[ -f "$HERE/icon.png" ] && install -m 644 "$HERE/icon.png" "$ICON_DIR/acord.png"

cat > "$APP_DIR/acord.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Acord
Comment=Native markdown editor with Cordial expressions and tables
Exec=$BIN_DIR/Acord %F
Icon=acord
Terminal=false
Categories=Utility;TextEditor;Office;
MimeType=text/markdown;text/plain;
DESKTOP

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi

echo "Installed:"
echo "  binary  → $BIN_DIR/Acord"
echo "  icon    → $ICON_DIR/acord.png"
echo "  desktop → $APP_DIR/acord.desktop"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) echo "note: $BIN_DIR is not on your PATH" >&2 ;;
esac
INSTALLER_EOF
    chmod +x "$stage/install.sh"

    zip_target "linux-${arch}" "$stage"
}

echo "packaging: ${TARGETS[*]}"
echo

for t in "${TARGETS[@]}"; do
    case "$t" in
        macos-aarch64)   build_macos aarch64 ;;
        macos-x86_64)    build_macos x86_64 ;;
        windows-aarch64) build_windows aarch64 ;;
        windows-x86_64)  build_windows x86_64 ;;
        linux-aarch64)   build_linux aarch64 ;;
        linux-x86_64)    build_linux x86_64 ;;
        *) echo "unknown target: $t (valid: ${ALL_TARGETS[*]})" >&2; exit 2 ;;
    esac
done

echo
echo "done. dist:"
ls -lh "$DIST"
