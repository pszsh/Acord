#!/usr/bin/env bash
set -euo pipefail

# Debug build — same wiring as build.sh but unoptimised, with -g, and
# launched in the foreground so Rust panics print straight to this terminal.

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Linux) ;;
    *) echo "wrong platform: $(uname -s) — use cargo xtask debug" >&2; exit 1;;
esac

export RUST_BACKTRACE=1

if [ -n "${ACORD_FEATURES:-}" ]; then
    cargo build -p acord-linux --no-default-features --features "$ACORD_FEATURES"
else
    cargo build -p acord-linux
fi

EXE="$ROOT/target/debug/acord"

# Rasterize the icon next to the exe so the dev binary has a window icon too.
if command -v rsvg-convert >/dev/null 2>&1 && [ -f "$ROOT/assets/Acord.svg" ]; then
    rsvg-convert --width 256 --height 256 "$ROOT/assets/Acord.svg" -o "$ROOT/target/debug/icon.png"
fi

pkill -x acord 2>/dev/null || true
sleep 0.3

echo
echo "Launching $EXE — Rust panics will print below."
echo "(Ctrl+C to exit, or quit Acord normally.)"
echo "----------------------------------------------------------"
exec "$EXE"
