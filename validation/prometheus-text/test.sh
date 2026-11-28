#!/usr/bin/env bash
set -euo pipefail
tree=$(cargo tree --locked -p prometheus --no-default-features -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$tree" | rg '^protobuf(-support)? v'; then
    echo 'Protobuf must not be in the text-only dependency graph' >&2
    exit 1
fi
cargo test --locked -p prometheus --lib --no-default-features encoder::text::tests
cargo test --locked -p prometheus --lib encoder::text::tests
