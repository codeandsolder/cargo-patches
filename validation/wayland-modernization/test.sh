#!/usr/bin/env bash
set -euo pipefail
cat > smoke.rs <<'RUST'
pub fn clipboard_for_display(display: &winit::event_loop::OwnedDisplayHandle) -> std::io::Result<Option<smithay_clipboard::Clipboard>> {
    display.wayland_connection().map(smithay_clipboard::Clipboard::from_connection).transpose()
}
RUST
cargo test --locked -p wayland-ownership-validation --lib --no-run
tree=$(cargo tree --locked -p wayland-ownership-validation -e normal,build --prefix none --format '{p}')
if rg '^(sctk-adwaita|ab_glyph|owned_ttf_parser|skrifa|harfrust) v' <<< "$tree"; then
    echo 'Native event translation selected an unused client decoration engine' >&2
    exit 1
fi
cargo test --locked -p sctk-adwaita --no-default-features --features skrifa --lib
cargo test --locked -p wayland-ownership-validation --features title-font --lib --no-run
tree=$(cargo tree --locked -p wayland-ownership-validation --features title-font -e normal,build --prefix none --format '{p}')
if rg '^(ttf-parser|ab_glyph|owned_ttf_parser|crossfont|cosmic-text) v' <<< "$tree"; then
    echo 'Client title rendering selected a superseded font renderer' >&2
    exit 1
fi
rg '^skrifa v0\.49\.' <<< "$tree"
rg '^harfrust v0\.15\.' <<< "$tree"
test "$(rg '^read-fonts v' <<< "$tree" | sort -u | wc -l)" -eq 1
rg '^read-fonts v0\.46\.' <<< "$tree"
