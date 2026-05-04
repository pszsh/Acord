#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

# debug build: skip release flags by reusing build.sh but with the dev profile.
# build.sh always uses release; for now, point users to install.sh for normal use,
# and stream the simulator log for the bundle id.
bash "$ROOT/scripts/ios/install.sh"
xcrun simctl spawn booted log stream --predicate 'subsystem == "org.else-if.acord" OR processImagePath CONTAINS "Acord"' --level debug
