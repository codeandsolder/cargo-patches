#!/usr/bin/env bash
set -euo pipefail
tree=$(cargo tree --locked -p rtime-clock --no-default-features -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$tree" | rg '^(nix|rtime-core|thiserror|tracing|serde)( |$)'; then
    echo 'Standalone adjtime wrappers unexpectedly require clock backend dependencies' >&2
    exit 1
fi
cargo test --locked -p rtime-clock --no-default-features --lib
cargo test --locked -p rtime-clock --all-features --lib
cargo doc --locked -p rtime-clock --no-default-features --no-deps
