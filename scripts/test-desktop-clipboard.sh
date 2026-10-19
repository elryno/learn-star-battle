#!/usr/bin/env bash
# Interactive desktop check: first copy tests/fixtures/10x10-daily-page.png from
# a browser or screenshot tool. Only reads the clipboard; saves are isolated.
set -euo pipefail
cd "$(dirname "$0")/.."
test -n "${WAYLAND_DISPLAY:-}"
output="$PWD/artifacts/desktop-clipboard"
mkdir -p "$output"
state=$(mktemp -d "$output/state.XXXXXX")
env -u STAR_BATTLE_TEST_BOARD -u STAR_BATTLE_TEST_UNIQUE \
    GDK_BACKEND=wayland GTK_A11Y=none GSK_RENDERER=cairo XDG_DATA_HOME="$state" \
    STAR_BATTLE_CAPTURE_PATH="$output/${1:-clipboard}.png" STAR_BATTLE_CAPTURE_SCENARIO=clipboard \
    timeout 30s target/debug/star-battle-solver
