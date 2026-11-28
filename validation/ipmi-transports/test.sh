#!/usr/bin/env bash
set -euo pipefail
tree=$(cargo tree --locked -p ipmi-rs --no-default-features --features unix-file -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$tree" | rg '^(aes|cbc|hmac|sha1|md5|rand|rand_core|rand_chacha|getrandom|time|bitflags v1|nix v0\.30)( |\.|$)'; then
    echo 'Local IPMI transport unexpectedly requires remote crypto, date formatting or superseded dependency versions' >&2
    exit 1
fi
cargo check --locked -p ipmi-rs --no-default-features --lib
cargo check --locked -p ipmi-rs --no-default-features --features unix-file --lib
cargo test --locked -p ipmi-rs --no-default-features --features rmcp --lib
cargo test --locked -p ipmi-rs --all-features --lib
cargo check --locked -p ipmi-rs --all-features --examples
cargo test --locked -p ipmi-rs-core --lib
