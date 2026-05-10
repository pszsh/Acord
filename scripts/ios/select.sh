#!/usr/bin/env bash
set -euo pipefail

# Interactive picker for the iOS deploy target. Lists every paired physical
# device and every available iPad simulator, lets the user pick one, then
# stores the choice at $HOME/.acord/ios-target so install.sh / debug.sh can
# read it back without prompting again.

CONFIG_DIR="$HOME/.acord"
CONFIG_FILE="$CONFIG_DIR/ios-target"
mkdir -p "$CONFIG_DIR"

ALL_FILE="$(mktemp)"
trap 'rm -f "$ALL_FILE"' EXIT

echo "Scanning paired devices..."
# devicectl columns are aligned with 2+ spaces. fields: name | host | uuid | state | model
xcrun devicectl list devices 2>/dev/null \
    | awk -F'  +' '/available \(paired\)/ {
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $1)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $3)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", $5)
        if ($3 ~ /^[A-F0-9-]{36}$/) print "device|" $3 "|" $1 " — " $5
    }' >> "$ALL_FILE" || true

echo "Scanning iPad simulators..."
# simctl line: "    iPad Pro 11-inch (M4) (UUID) (Shutdown)"
# strip whitespace, peel off "(state)" then "(uuid)" from the right.
xcrun simctl list devices available 2>/dev/null \
    | awk '/iPad/ {
        line=$0
        sub(/^[[:space:]]+/, "", line); sub(/[[:space:]]+$/, "", line)
        if (match(line, /\([^()]+\)$/)) {
            state=substr(line, RSTART+1, RLENGTH-2)
            line=substr(line, 1, RSTART-1)
            sub(/[[:space:]]+$/, "", line)
        } else { state="" }
        if (match(line, /\([A-F0-9-]{36}\)$/)) {
            uuid=substr(line, RSTART+1, 36)
            name=substr(line, 1, RSTART-1)
            sub(/[[:space:]]+$/, "", name)
        } else { next }
        print "sim|" uuid "|" name " (" state ")"
    }' >> "$ALL_FILE" || true

COUNT=$(wc -l < "$ALL_FILE" | tr -d ' ')
if [ "$COUNT" -eq 0 ]; then
    echo "no paired devices and no iPad simulators found" >&2
    echo "  - connect an iPad via cable and trust this Mac" >&2
    echo "  - or open Xcode → Window → Devices and Simulators to add a sim" >&2
    exit 1
fi

echo
echo "available iOS targets:"
i=1
while IFS= read -r entry; do
    [ -z "$entry" ] && continue
    IFS='|' read -r kind id label <<< "$entry"
    printf "  %2d) [%-6s] %s\n" "$i" "$kind" "$label"
    i=$((i + 1))
done < "$ALL_FILE"

echo
read -r -p "pick a target (number): " CHOICE
if ! [[ "$CHOICE" =~ ^[0-9]+$ ]] || [ "$CHOICE" -lt 1 ] || [ "$CHOICE" -gt "$COUNT" ]; then
    echo "invalid choice: $CHOICE" >&2
    exit 1
fi

PICK=$(sed -n "${CHOICE}p" "$ALL_FILE")
IFS='|' read -r KIND ID LABEL <<< "$PICK"

cat > "$CONFIG_FILE" <<EOF
KIND=$KIND
ID=$ID
LABEL=$LABEL
EOF

echo "saved $CONFIG_FILE: $KIND $ID ($LABEL)"
