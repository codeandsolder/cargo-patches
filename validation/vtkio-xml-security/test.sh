#!/usr/bin/env bash
# Validate the actual generated sources, including legacy and XML round trips.
set -euo pipefail
cargo test --locked -p quick-xml --features serialize --lib
cargo test --locked -p vtkio --no-default-features --features xml
