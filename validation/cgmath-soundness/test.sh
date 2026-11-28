#!/usr/bin/env bash
# Run from the disposable patched family workspace.
set -euo pipefail
cargo test --locked -p cgmath --features serde
