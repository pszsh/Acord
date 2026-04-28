#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Linux) ;;
    *) echo "wrong platform: $(uname -s) — use cargo xtask build" >&2; exit 1;;
esac

# Pick the winit backend(s). Default builds enable both x11 and wayland so a
# single binary works on either; ACORD_FEATURES overrides for cases where
# only one backend is available (flatpak, stripped distros, debugging one
# backend in isolation).
detect_features() {
    if [ -n "${ACORD_FEATURES:-}" ]; then
        echo "$ACORD_FEATURES"
        return
    fi
    # No detection — both backends are linked by default. Override only when
    # you need to force one.
    echo ""
}

FEATURES="$(detect_features)"
echo "build: XDG_CURRENT_DESKTOP=${XDG_CURRENT_DESKTOP:-<unset>}, WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-<unset>}"

if [ -n "$FEATURES" ]; then
    echo "build: forcing features = $FEATURES"
    cargo build --release -p acord-linux --no-default-features --features "$FEATURES"
else
    echo "build: linking both x11 and wayland backends"
    cargo build --release -p acord-linux
fi

STAGE="$ROOT/build/bin"
mkdir -p "$STAGE"

cp "$ROOT/target/release/acord" "$STAGE/Acord"
chmod +x "$STAGE/Acord"

# Rasterize the SVG icon next to the binary so load_window_icon picks it up.
if command -v rsvg-convert >/dev/null 2>&1 && [ -f "$ROOT/assets/Acord.svg" ]; then
    rsvg-convert --width 256 --height 256 "$ROOT/assets/Acord.svg" -o "$STAGE/icon.png"
else
    echo "rsvg-convert not found or assets/Acord.svg missing — skipping icon"
fi

[ -f "$ROOT/LICENCE" ] && cp "$ROOT/LICENCE" "$STAGE/LICENCE"

echo "Built: $STAGE/Acord"
