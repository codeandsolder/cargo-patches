#!/usr/bin/env bash
# Called in the disposable, fully patched family workspace.
set -euo pipefail
metadata=$(cargo metadata --locked --all-features --filter-platform x86_64-unknown-linux-gnu --format-version 1)
jq -e '
  .packages as $p |
  all(["reqsign-core","reqsign-google","reqsign-azure-storage"][];
      . as $name | [$p[]|select(.name==$name)] |
      length==1 and .[0].source==null) and
  ([$p[]|select(.name=="btls-sys")]|length==1)
' <<<"$metadata"
# All provider library tests must link against the same patched Reqsign and
# BTLS. Test-only upstream helper versions remain pinned in this workspace.
cargo test --locked --workspace --all-features --features btls/static-cxx-runtime --lib --no-run --message-format=json > test-artifacts.jsonl
for crate in reqsign_core reqsign_google reqsign_azure_storage; do
    binary=$(jq -er --arg name "$crate" 'select(.reason=="compiler-artifact" and .target.name==$name and .profile.test and .executable!=null)|.executable' test-artifacts.jsonl)
    [[ -f "$binary" ]]
    needed=$(readelf -W --dynamic "$binary")
    if rg -q '\(NEEDED\).*\[(libstdc\+\+|libc\+\+|libc\+\+abi|libunwind)\.so' <<<"$needed"; then
        echo "Unexpected shared C++ runtime in $crate" >&2
        exit 1
    fi
    case "$crate" in
      reqsign_core) "$binary" jwt:: ;;
      reqsign_google) "$binary" sign_request::tests:: ;;
      reqsign_azure_storage) "$binary" sign_request::tests:: ;;
    esac
done
