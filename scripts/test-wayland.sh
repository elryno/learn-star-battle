#!/usr/bin/env bash
# Real Wayland clipboard, pointer, and fractional-scale tests on a private compositor.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
if [[ $# -gt 0 ]]; then export STAR_BATTLE_TEST_SCALES="$*"; fi
if [[ ${STAR_BATTLE_WAYLAND_DBUS:-0} != 1 ]]; then
    mkdir -p "$root/artifacts/wayland-review"
    # Unix socket paths are short; a CI checkout path can already fill the limit.
    # Keep runtime sockets in /tmp while preserving logs under artifacts/.
    export XDG_RUNTIME_DIR
    XDG_RUNTIME_DIR=$(mktemp -d /tmp/starbattle-wayland.XXXXXX)
    export GIO_USE_VFS=local GTK_USE_PORTAL=0
    exec dbus-run-session -- env STAR_BATTLE_WAYLAND_DBUS=1 bash "$root/scripts/test-wayland.sh"
fi
local_tools="$root/.cache/native-test-tools/root/usr"
if [[ -x "$local_tools/bin/sway" ]]; then
    export PATH="$local_tools/bin:$PATH"
    export LD_LIBRARY_PATH="$local_tools/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
for tool in sway swaymsg wl-copy wtype grim wayland-scanner; do command -v "$tool" >/dev/null; done
output="$root/artifacts/wayland-review"
wayland-scanner client-header tests/support/wlr-virtual-pointer-unstable-v1.xml "$output/virtual-pointer.h"
wayland-scanner private-code tests/support/wlr-virtual-pointer-unstable-v1.xml "$output/virtual-pointer.c"
cc -Wall -Wextra -Werror -I"$output" tests/support/wayland-pointer.c "$output/virtual-pointer.c" -o "$output/pointer" $(pkg-config --cflags --libs wayland-client)
cat > "$XDG_RUNTIME_DIR/sway.conf" <<'SWAY'
xwayland disable
output HEADLESS-1 mode 1920x1200
seat seat0 fallback true
seat seat0 attach *
focus_follows_mouse no
default_border none
for_window [app_id="io.github.elryno.LearnStarBattle"] floating enable
SWAY
export WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1
export GDK_BACKEND=wayland GTK_A11Y=none GSK_RENDERER=cairo
unset DISPLAY WAYLAND_DISPLAY SWAYSOCK
sway --unsupported-gpu --config "$XDG_RUNTIME_DIR/sway.conf" > "$output/compositor.log" 2>&1 &
compositor=$!
trap 'kill "$compositor" 2>/dev/null || true' EXIT
export SWAYSOCK="$XDG_RUNTIME_DIR/sway-ipc.$UID.$compositor.sock"
for _ in {1..100}; do
    if [[ -S "$SWAYSOCK" ]]; then break; fi
    kill -0 "$compositor" || { cat "$output/compositor.log"; exit 1; }
    sleep 0.1
done
for socket in "$XDG_RUNTIME_DIR"/wayland-*; do
    if [[ -S "$socket" ]]; then export WAYLAND_DISPLAY="$socket"; break; fi
done
test -n "${WAYLAND_DISPLAY:-}"
state=$(mktemp -d "$output/state.XXXXXX")
if ! python3 scripts/test-wayland.py "$state" "$output" > "$output/input.log" 2>&1; then
    cat "$output/input.log"
    exit 1
fi
# Sway can lack xdg_popup.reposition; GTK remaps the popup instead. Keep that
# capability warning in the evidence, but fail on every other warning/error.
# GTK fallback: https://mail.gnome.org/archives/commits-list/2020-April/msg04054.html
fallback="Gdk-WARNING .*: Compositor doesn't support moving popups, relying on remapping$"
if rg -n 'CRITICAL|WARNING|panicked' "$output/app.log" | rg -v "$fallback"; then exit 1; fi
if rg -q "$fallback" "$output/app.log"; then
    printf 'NOTE: GTK used its popup-remapping fallback on the test compositor.\n'
fi
cat "$output/results.txt"
