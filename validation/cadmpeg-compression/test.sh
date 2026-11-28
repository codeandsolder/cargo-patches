#!/usr/bin/env bash
set -euo pipefail
cargo test --locked --workspace
tree=$(cargo tree --locked -e normal,build)
if rg -q "miniz_oxide" <<< "$tree"; then
    echo "unexpected miniz_oxide backend" >&2
    exit 1
fi
