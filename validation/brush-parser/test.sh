#!/usr/bin/env bash
set -euo pipefail
cargo test --locked -p brush-parser --lib -- --skip snapshot_tests
