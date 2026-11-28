#!/usr/bin/env bash
set -euo pipefail
tree=$(cargo tree --locked -p ipmi-rs --no-default-features --features unix-file -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$tree" | rg '^(aes|cbc|hmac|sha1|md5|rand|rand_core|rand_chacha|getrandom|time|bitflags v1|nix)( |\.|$)'; then
    echo 'Local IPMI transport unexpectedly requires remote crypto, date formatting or superseded dependency versions' >&2
    exit 1
fi
cargo check --locked -p ipmi-rs --no-default-features --lib
# Verify actual kernel UAPI layouts and opcodes rather than architecture-specific constants.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
cc -std=c11 -Wall -Werror "$(dirname "$0")/abi.c" -o "$scratch/abi"
actual=$(cargo test --locked -p ipmi-rs --no-default-features --features unix-file --lib -- --nocapture)
printf '%s\n' "$actual"
expected=$("$scratch/abi")
[[ $(printf '%s\n' "$actual" | rg -o 'IPMI_ABI [0-9 ]+') == "$expected" ]] || { echo "IPMI ABI mismatch: expected $expected" >&2; exit 1; }
cargo test --locked -p ipmi-rs --no-default-features --features rmcp --lib
cargo test --locked -p ipmi-rs --all-features --lib
cargo check --locked -p ipmi-rs --all-features --examples
cargo test --locked -p ipmi-rs-core --lib
