#!/usr/bin/env bash
# Native GTK regression scenarios, isolated from the player's session and saves.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
if [[ ${STAR_BATTLE_TEST_DBUS:-0} != 1 ]]; then
    mkdir -p "${STAR_BATTLE_TEST_OUTPUT:-$root/artifacts/native-review}"
    export XDG_RUNTIME_DIR
    XDG_RUNTIME_DIR=$(mktemp -d "${STAR_BATTLE_TEST_OUTPUT:-$root/artifacts/native-review}/runtime.XXXXXX")
    XDG_RUNTIME_DIR=$(realpath "$XDG_RUNTIME_DIR")
    export GIO_USE_VFS=local GTK_USE_PORTAL=0
    exec dbus-run-session -- env STAR_BATTLE_TEST_DBUS=1 bash "$root/scripts/test-native.sh" "$@"
fi
local_tools="$root/.cache/native-test-tools/root/usr"
if [[ -x "$local_tools/bin/Xvfb" ]]; then
    export PATH="$local_tools/bin:$PATH"
    export LD_LIBRARY_PATH="$local_tools/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
fi
command -v Xvfb >/dev/null
output="${STAR_BATTLE_TEST_OUTPUT:-$root/artifacts/native-review}"
mkdir -p "$output"
output=$(realpath "$output")
runtime=$XDG_RUNTIME_DIR
export GDK_BACKEND=x11 GTK_A11Y=none GSK_RENDERER=cairo
Xvfb -displayfd 3 -screen 0 1400x1100x24 -nolisten tcp -noreset 3>"$runtime/display" >"$output/xvfb.log" 2>&1 &
xvfb_pid=$!
trap 'kill "$xvfb_pid" 2>/dev/null || true' EXIT
display_ready=0
for _ in {1..100}; do
    if ! kill -0 "$xvfb_pid" 2>/dev/null; then
        cat "$output/xvfb.log"
        printf 'Xvfb exited before the test display was ready.\n' >&2
        exit 1
    fi
    if [[ -s "$runtime/display" ]]; then
        export DISPLAY=":$(cat "$runtime/display")"
        if xdotool getdisplaygeometry >/dev/null 2>&1; then
            display_ready=1
            break
        fi
    fi
    sleep 0.1
done
if [[ "$display_ready" != 1 ]]; then
    cat "$output/xvfb.log"
    printf 'Xvfb did not provide a usable test display within 10 seconds.\n' >&2
    exit 1
fi
scenarios=("$@")
if [[ ${#scenarios[@]} -eq 0 ]]; then
    scenarios=(teaching teaching-compact teaching-narrow teaching-dark layout compact narrow hint timeout preview counting correction correction-preview retained-hint retained-correction auto drag errors error-toggle new-puzzle solved import source-align source-align-compact source-align-narrow help about external)
fi
failures=0
: > "$output/results.txt"
for scenario in "${scenarios[@]}"; do
    state=$(mktemp -d "$output/$scenario-state.XXXXXX")
    if [[ "$scenario" == external ]]; then
        if python3 scripts/test-input.py "$state" "$output" >"$output/external.log" 2>&1 && ! rg -q "CRITICAL|WARNING|panicked" "$output/external-app.log"; then
            printf 'PASS external mouse/keyboard/clipboard/restart
' | tee -a "$output/results.txt"
        else
            printf 'FAIL external (see external.log)
' | tee -a "$output/results.txt"
            failures=$((failures + 1))
        fi
        continue
    fi
    board="$root/tests/fixtures/10x10-reported-position.json"
    args=()
    case "$scenario" in
        solved) board="$root/tests/fixtures/10x10-almost-solved.json" ;;
        counting) board="$root/tests/fixtures/10x10-counting-position.json" ;;
        import|source-align) board=; args=("$root/tests/fixtures/10x10-daily-page.png") ;;
        source-align-compact) board=; args=("$root/tests/fixtures/14x14-partial.jpg") ;;
        source-align-narrow) board=; args=("$root/tests/fixtures/5x5-dark-partial.jpg") ;;
    esac
    command_env=(env -u STAR_BATTLE_TEST_BOARD XDG_DATA_HOME="$state" STAR_BATTLE_CAPTURE_PATH="$output/$scenario.png" STAR_BATTLE_CAPTURE_SCENARIO="$scenario")
    [[ -z "$board" ]] || command_env+=(STAR_BATTLE_TEST_BOARD="$board")
    runner=(timeout 40s)
    if [[ ${STAR_BATTLE_TEST_DEBUGGER:-0} == 1 ]]; then
        command_env+=(G_DEBUG=fatal-criticals)
        runner+=(gdb -q -batch -ex run -ex 'bt 20' --args)
    fi
    if "${command_env[@]}" "${runner[@]}" "$root/target/debug/star-battle-solver" "${args[@]}" >"$output/$scenario.log" 2>&1; then
        if rg -q 'CRITICAL|WARNING|panicked' "$output/$scenario.log"; then
            printf 'WARN %s (see %s.log)\n' "$scenario" "$scenario" | tee -a "$output/results.txt"
            failures=$((failures + 1))
        else
            printf 'PASS %s\n' "$scenario" | tee -a "$output/results.txt"
        fi
    else
        printf 'FAIL %s (see %s.log)\n' "$scenario" "$scenario" | tee -a "$output/results.txt"
        failures=$((failures + 1))
    fi
done
exit "$failures"
