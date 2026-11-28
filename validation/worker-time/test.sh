#!/usr/bin/env bash
set -euo pipefail
rustup target add wasm32-unknown-unknown
tree=$(cargo tree --locked -p worker --target wasm32-unknown-unknown -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$tree" | rg '^chrono(-tz)? v'; then
    echo "Chrono must not be in the default Worker graph" >&2
    exit 1
fi
cargo check --locked -p worker --target wasm32-unknown-unknown
cargo check --locked -p worker --features timezone --target wasm32-unknown-unknown
