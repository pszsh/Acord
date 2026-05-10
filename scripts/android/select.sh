#!/usr/bin/env bash
set -euo pipefail

# Stub — the Android shell hasn't been started yet.
# When it lands, this should mirror scripts/ios/select.sh: list `adb devices`
# entries plus available emulators (`emulator -list-avds`), let the user
# pick one, and write the choice to $HOME/.acord/android-target.

cat <<'EOF' >&2
android select is a stub — the Android shell isn't built yet.

when it ships, this will:
  - list connected devices (adb devices)
  - list available emulators (emulator -list-avds)
  - write the picked target to $HOME/.acord/android-target
EOF
exit 1
