#!/usr/bin/env bash
# Generates ios/Assets.xcassets/AppIcon.appiconset/ from assets/Acord.svg.
# Used by both the CLI build (build.sh) and the Xcode project path
# (xcodeproj.sh). Idempotent — re-running just overwrites.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SVG="$ROOT/assets/Acord.svg"
ASSETS="$ROOT/ios/Assets.xcassets"
APPICON="$ASSETS/AppIcon.appiconset"

if [ ! -f "$SVG" ]; then
    echo "ERROR: $SVG not found" >&2
    exit 1
fi

if ! command -v rsvg-convert >/dev/null 2>&1; then
    echo "ERROR: rsvg-convert not on PATH (brew install librsvg or port install librsvg)" >&2
    exit 1
fi

mkdir -p "$APPICON"

# (filename, pixel size) pairs covering iPhone + iPad icon slots through iOS 17.
# 1024 is the marketing icon; the rest are point-size@scale variants.
SIZES=(
    "Icon-20.png 20"
    "Icon-20@2x.png 40"
    "Icon-20@3x.png 60"
    "Icon-29.png 29"
    "Icon-29@2x.png 58"
    "Icon-29@3x.png 87"
    "Icon-40.png 40"
    "Icon-40@2x.png 80"
    "Icon-40@3x.png 120"
    "Icon-60@2x.png 120"
    "Icon-60@3x.png 180"
    "Icon-76.png 76"
    "Icon-76@2x.png 152"
    "Icon-83.5@2x.png 167"
    "Icon-1024.png 1024"
)

for entry in "${SIZES[@]}"; do
    name="${entry%% *}"
    size="${entry##* }"
    rsvg-convert --width="$size" --height="$size" "$SVG" -o "$APPICON/$name"
done

# Top-level Assets.xcassets/Contents.json (xcode requires it even if empty-ish).
cat > "$ASSETS/Contents.json" <<'EOF'
{
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
EOF

# AppIcon.appiconset/Contents.json — maps every Icon-*.png to its slot.
cat > "$APPICON/Contents.json" <<'EOF'
{
  "images" : [
    { "idiom" : "iphone",      "size" : "20x20",   "scale" : "2x", "filename" : "Icon-20@2x.png" },
    { "idiom" : "iphone",      "size" : "20x20",   "scale" : "3x", "filename" : "Icon-20@3x.png" },
    { "idiom" : "iphone",      "size" : "29x29",   "scale" : "2x", "filename" : "Icon-29@2x.png" },
    { "idiom" : "iphone",      "size" : "29x29",   "scale" : "3x", "filename" : "Icon-29@3x.png" },
    { "idiom" : "iphone",      "size" : "40x40",   "scale" : "2x", "filename" : "Icon-40@2x.png" },
    { "idiom" : "iphone",      "size" : "40x40",   "scale" : "3x", "filename" : "Icon-40@3x.png" },
    { "idiom" : "iphone",      "size" : "60x60",   "scale" : "2x", "filename" : "Icon-60@2x.png" },
    { "idiom" : "iphone",      "size" : "60x60",   "scale" : "3x", "filename" : "Icon-60@3x.png" },
    { "idiom" : "ipad",        "size" : "20x20",   "scale" : "1x", "filename" : "Icon-20.png" },
    { "idiom" : "ipad",        "size" : "20x20",   "scale" : "2x", "filename" : "Icon-20@2x.png" },
    { "idiom" : "ipad",        "size" : "29x29",   "scale" : "1x", "filename" : "Icon-29.png" },
    { "idiom" : "ipad",        "size" : "29x29",   "scale" : "2x", "filename" : "Icon-29@2x.png" },
    { "idiom" : "ipad",        "size" : "40x40",   "scale" : "1x", "filename" : "Icon-40.png" },
    { "idiom" : "ipad",        "size" : "40x40",   "scale" : "2x", "filename" : "Icon-40@2x.png" },
    { "idiom" : "ipad",        "size" : "76x76",   "scale" : "1x", "filename" : "Icon-76.png" },
    { "idiom" : "ipad",        "size" : "76x76",   "scale" : "2x", "filename" : "Icon-76@2x.png" },
    { "idiom" : "ipad",        "size" : "83.5x83.5","scale" : "2x","filename" : "Icon-83.5@2x.png" },
    { "idiom" : "ios-marketing","size" : "1024x1024","scale" : "1x","filename" : "Icon-1024.png" }
  ],
  "info" : {
    "author" : "xcode",
    "version" : 1
  }
}
EOF

echo "Wrote $APPICON ($(ls "$APPICON" | wc -l | tr -d ' ') files)"
