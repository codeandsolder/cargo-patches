# Cargo patch queues

Main is the reviewed source of truth: pinned upstream commits, minimal patches, and import tooling.
CI checks every queue on pull requests, then imports and publishes changed families after main merges.
Generated branches are machine-owned, fast-forward only, and each changed snapshot receives a new immutable family tag. Never edit generated branches or move tags.

Packages requiring the same Cargo type identity belong in one family snapshot. Netty's sibling HTTP/2 path dependency therefore resolves to the same Git source as Wreq's direct HTTP/2 dependency.
Library dependencies that require coordinated package identities use explicit family tags, not main, and group all references to the same family in one Renovate PR. Renovate's default Git SHA tracking targets the default branch and is unsuitable here; use native Cargo tag updates with a family-specific versionCompatibility filter (preserves the full tag). Use the shared preset `github>codeandsolder/cargo-patches:renovate-consumer`. Utility tags are intentionally not updated independently, because the utility pins Wreq and must share its native TLS provider. Consumer CI must validate builds and tests before adopting a tag. CI validates source import, exact patch application and Cargo manifest discovery; families with a checked workspace in `validation/<family>` also run their native or semantic regression gate against the actual staged sources. Validation workspaces never enter the generated branch; the consumer integration gate remains mandatory.
External upstream revision changes require reviewed updates and the fleet's 72-hour observation policy. The initial queues use the exact historical vendored bases. No automatic unreviewed upstream bump is enabled.

## Workspace-root patch overrides

For an application or workspace-root `[patch]` override, use the protected `generated/<family>` branch instead of a tag. Commit the Cargo.lock resolution: normal locked builds retain the exact published commit, while Renovate lockfile maintenance can advance it through Cargo without extracting or rewriting the patch table. Consumer CI must verify the intended patched source remains selected; a registry version may otherwise supersede it. Do not mix branch and tag references for the same type-sharing family in one dependency graph.

Example:

```toml
[patch.crates-io]
surrealdb-diskann = { git = "https://github.com/codeandsolder/cargo-patches.git", branch = "generated/surrealdb-diskann" }
```

## Reqsign native signing family

The `reqsign-crypto` queue contains the exact registry versions of Reqsign core,
Azure Storage and Google used by sccache. It replaces their RustCrypto RSA
signing with btls-fast while preserving RS256, JWT header/claim shapes and
Google's uppercase hexadecimal signed-URL signatures. It shares the native
RSA key loader and signing primitive; the full native family must be patched
together because the core signer now accepts a btls key. This queue targets
native consumers; it is not a WASM replacement. The explicit btls revision must
match the consumer's Wreq and other btls dependencies. Update the family pin and
consumer together when changing btls. Do not manually edit the generated branch.

The pure Rust SHA/HMAC helpers are retained; they are also used for non-RSA
request signing and the consumer still needs hashing for other operations.

Explicit `exclude_files` entries omit identified upstream files that are not
required for the published package. Each entry specifies a package-relative
path and SHA-256 of the exact upstream file. Import and publication both verify
the file before removing it, so an upstream change requires review. Reqsign
Google's unused testbucket credential fixture is omitted; no push-protection
exception is needed.

The Reqsign validation workspace pins the original test-helper dependency versions to retain one patched core identity. It links all three provider library test binaries with the static BTLS runtime, inspects native dependencies, and runs JWT/signing regressions before publication. Update its lock alongside any family dependency pin change.

## OAR OCR feature ownership

The `oar-image-features` family imports the two matching 0.11.0 registry
crates. `image-formats` is independently selectable (enabled by the
standalone defaults); an RGB-buffer consumer can disable defaults and
select only its own encoded formats through `image`. Both OAR packages
retain Rayon for image processing, without the unused imageproc text
and FFT subsystems. ZIP explicitly selects its own zlib-rs backend so
Indra NPZ loading does not depend on accidental PNG/TIFF feature
unification. Recognition algorithms and model provisioning are unchanged.
The validation workspace exercises raw RGB, narrow PNG/PNM, and full
format configurations through the actual OAR utility APIs.
