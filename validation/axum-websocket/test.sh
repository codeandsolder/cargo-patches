#!/usr/bin/env bash
set -euo pipefail
cargo test --locked -p axum --features ws,http2,axum-macros/__private --lib extract::ws
