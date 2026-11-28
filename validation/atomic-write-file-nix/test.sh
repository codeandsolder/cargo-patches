#!/usr/bin/env bash
set -euo pipefail
cargo test --locked -p atomic-write-file
cargo test --locked -p atomic-write-file --features unnamed-tmpfile
