#!/usr/bin/env bash
# Sourced by build / install / debug scripts to redirect cargo's target dir
# to the boot SSD instead of the repo's external spinning disk.
# Scripts read compiled artifacts from $CARGO_TARGET_DIR and copy the final
# .app / .exe / binary into $ROOT/build/ as real files at the end.
#
# Override the SSD location with: export CARGO_TARGET_DIR=/some/other/path

if [ -n "${ACORD_BUILD_DIRS_DONE:-}" ]; then
    return 0 2>/dev/null || exit 0
fi
export ACORD_BUILD_DIRS_DONE=1

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/acord/target}"
mkdir -p "$CARGO_TARGET_DIR"
