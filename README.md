# Cargo patch queues

Main is the reviewed source of truth: pinned upstream commits, minimal patches, and import tooling.
CI checks every queue on pull requests, then imports and publishes changed families after main merges.
Generated branches are machine-owned, fast-forward only, and each changed snapshot receives a new immutable family tag. Never edit generated branches or move tags.

Packages requiring the same Cargo type identity belong in one family snapshot. Netty's sibling HTTP/2 path dependency therefore resolves to the same Git source as Wreq's direct HTTP/2 dependency.
Consumers use explicit family tags, not main, and group all references to the same family in one Renovate PR. Renovate's default Git SHA tracking targets the default branch and is unsuitable here; use native Cargo tag updates with a family-specific versionCompatibility filter (preserves the full tag). Use the shared preset `github>codeandsolder/cargo-patches:renovate-consumer`. Utility tags are intentionally not updated independently, because the utility pins Wreq and must share its native TLS provider. Consumer CI must validate builds and tests before adopting a tag. CI validates source import, exact patch application and Cargo manifest discovery; it is not a substitute for consumer integration tests.
External upstream revision changes require reviewed updates and the fleet's 72-hour observation policy. The initial queues use the exact historical vendored bases. No automatic unreviewed upstream bump is enabled.
