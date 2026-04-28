#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

case "$(uname -s)" in
    Linux) ;;
    *) echo "wrong platform: $(uname -s) — use cargo xtask install" >&2; exit 1;;
esac

bash "$ROOT/scripts/linux/build.sh"

# XDG-correct install: binary into ~/.local/bin (PATH on most distros), icon
# + .desktop into ~/.local/share for the launcher menu.
BIN_DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/256x256/apps"

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"

# Kill running instance before replacing the binary.
pkill -x Acord 2>/dev/null || true
sleep 0.3

install -m 755 "$ROOT/build/bin/Acord" "$BIN_DIR/Acord"

if [ -f "$ROOT/build/bin/icon.png" ]; then
    install -m 644 "$ROOT/build/bin/icon.png" "$ICON_DIR/acord.png"
fi

cat > "$APP_DIR/acord.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Acord
Comment=Native markdown editor with Cordial expressions and tables
Exec=$BIN_DIR/Acord %F
Icon=acord
Terminal=false
Categories=Utility;TextEditor;Office;
MimeType=text/markdown;text/plain;
EOF

# Update the desktop database so the launcher picks up the new entry. Quiet
# fallback if the tool isn't installed.
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
