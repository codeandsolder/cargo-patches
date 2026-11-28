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

The `oar-image-features` family imports both matching OAR 0.11.0 registry
crates and their exact ORT 2.0.0-rc.15 library and bindings. `image-formats` is independently selectable (enabled by the
standalone defaults); an RGB-buffer consumer can disable defaults and
select only its own encoded formats through `image`. Both OAR packages
retain Rayon for image processing, without the unused imageproc text
and FFT subsystems. ZIP explicitly selects its own zlib-rs backend so
Indra NPZ loading does not depend on accidental PNG/TIFF feature
unification. Formula recognition and Indra are also independently selectable, enabled by
the standalone defaults. Ordinary text OCR, high-level layout and table
analysis do not need either pipeline. Formula-specific builder methods and
registry entries exist only when their feature is enabled. Shared reading
order helpers are independent of Indra. Recognition algorithms and model
provisioning are unchanged.
The validation workspace exercises raw RGB, narrow PNG/PNM, and full
format configurations through the actual OAR utility APIs.

ORT's `tls-rustls-platform` feature explicitly retains platform certificate
verification with Rustls for build-time binary downloads and optional runtime
model downloads. OAR selects this for `download-binaries`; its own runtime
downloader retains WebPKI verification. ORT uses the existing SHA-2 0.11
implementation for binary integrity and optional model cache keys. Family
validation exercises the actual TLS-selection and streaming hash helpers
without weakening certificate verification, and checks the dependency graph
for absence of native TLS and the separate HMAC/SHA-256 crate.

## Shared imaging glyph atlas

The `imaging-glyph-atlas` family imports the pinned Imaging workspace intact,
with explicitly checked package manifests, alongside exact registry Glifo and
Vello GPU versions. All consumers of Imaging's public types must select the
same family revision. Its regression gate exercises ordinary GPU glyph reuse,
subpixel placement, retained-scene isolation, and leased glyph residency on a
working WGPU adapter (Mesa Vulkan is sufficient). The atlas is populated on
first use; fixed-cell consumers can pin just their required glyph subset.

The Wayland modernization family also patches ui-events-winit 0.3.0 to
stop implicitly enabling Winit's default features. Event conversion must not
select the application windowing backends or a client-side decoration engine;
the window host explicitly selects X11, Wayland and raw-window-handle support.

The same family exposes Winit's owned Wayland connection for clipboard clients.
Smithay clipboard uses that connection without reconstructing a raw display
backend, reports worker initialization and selection errors, and preserves the
primary-selection protocol's independent availability. The validation workspace
checks the type boundary and that event translation cannot enable the old title
renderer.

The Wayland family uses one client-decoration text renderer: Skrifa 0.49
and Harfrust 0.15 sharing read-fonts 0.46, with Unicode bidi visual runs,
script itemization, native font instances and ink-bounds-aware rasterization.
Winit's ordinary decoration feature selects it; the title-free feature
remains independent. The obsolete ab_glyph, crossfont and cosmic-text
backend alternatives are removed in this family revision. The renderer
loads the selected collection face and named instance once and retains
the existing bundled-font fallback for unavailable system fonts.

The sctk-adwaita 0.11.1 registry archive excludes the upstream image
snapshot directory. The patch restores its unmodified expected PNGs
from upstream revision ea5939eca352d39fd0b7b9160fb2633da4bc6cc7
(the archive's VCS provenance). The original shaping fixture is generated
by validation/wayland-modernization/generate-font-fixture.py and has
known GPOS kerning, GSUB ligature and negative-bearing geometry.
The family gate runs the original pixel tests and the title tests, and
compiles both decorated and undecorated Winit/clipboard consumers.
