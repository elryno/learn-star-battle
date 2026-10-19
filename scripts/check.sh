#!/usr/bin/env bash
# One entry point for developer and CI verification; failures are never skipped.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z ${CARGO_HOME:-} && -d .cache/cargo/registry ]]; then
    export CARGO_HOME="$PWD/.cache/cargo"
fi
for script in scripts/*.sh; do bash -n "$script"; done
PYTHONPYCACHEPREFIX="$PWD/artifacts/python-cache" python3 -m py_compile scripts/test-input.py scripts/test-wayland.py
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-features --locked
cargo build --features capture --locked
bash scripts/test-native.sh
bash scripts/test-wayland.sh
cargo build --release --locked
PREFIX="$PWD/artifacts/install check" bash scripts/install.sh
printf 'All checks passed.\n'
