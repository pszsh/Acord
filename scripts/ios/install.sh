#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# pick the deploy target: explicit arg, or auto-detect (paired physical device wins).
TARGET="${1:-}"
if [ -z "$TARGET" ]; then
    if xcrun devicectl list devices 2>/dev/null | grep -q "available (paired)"; then
        TARGET="device"
    else
        TARGET="sim"
    fi
fi

case "$TARGET" in
    sim)
        bash "$ROOT/scripts/ios/build.sh" sim
        APP="$ROOT/build/ios/Acord.app"
        BUNDLE_ID="org.else-if.acord"

        DEV="$(xcrun simctl list devices booted | awk '/Booted/ {print $NF}' | tr -d '()' | head -1 || true)"
        if [ -z "$DEV" ]; then
            DEV="$(xcrun simctl list devices available | awk '/iPad/ && /\([A-F0-9\-]+\)/ {gsub(/[\(\)]/,"",$NF); print $NF; exit}')"
            if [ -z "$DEV" ]; then
                echo "no iPad simulator available — open Xcode → Window → Devices and Simulators to add one" >&2
                exit 1
            fi
            xcrun simctl boot "$DEV" 2>/dev/null || true
            open -a Simulator
        fi

        echo "Installing to simulator $DEV..."
        xcrun simctl install "$DEV" "$APP"
        echo "Launching..."
        xcrun simctl launch "$DEV" "$BUNDLE_ID"
        ;;

    device)
        bash "$ROOT/scripts/ios/build.sh" device
        APP="$ROOT/build/ios/Acord.app"

        DEVICE_ID="${ACORD_IOS_DEVICE:-}"
        if [ -z "$DEVICE_ID" ]; then
            DEVICE_ID="$(xcrun devicectl list devices 2>/dev/null \
                | awk '/available \(paired\)/ {for(i=1;i<=NF;i++) if($i ~ /^[A-F0-9-]{36}$/) {print $i; exit}}')"
        fi

        if [ -z "$DEVICE_ID" ]; then
            echo "no paired device found — connect via cable and trust this Mac on the device" >&2
            exit 1
        fi

        echo "Installing to device $DEVICE_ID..."
        xcrun devicectl device install app --device "$DEVICE_ID" "$APP"

        echo "Launching..."
        xcrun devicectl device process launch --device "$DEVICE_ID" org.else-if.acord || true
        ;;

    *)
        echo "usage: $0 [sim|device]" >&2
        exit 2
        ;;
esac
