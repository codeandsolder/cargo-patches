#!/usr/bin/env bash
set -euo pipefail
cat > smoke.rs <<'RUST'
pub fn clipboard_for_display(display: &winit::event_loop::OwnedDisplayHandle) -> std::io::Result<Option<smithay_clipboard::Clipboard>> {
    display.wayland_connection().map(smithay_clipboard::Clipboard::from_connection).transpose()
}
RUST
cargo check --locked --lib
if cargo tree --locked -e normal,build --prefix none --format '{p}' | rg '^(sctk-adwaita|ab_glyph|owned_ttf_parser) v'; then
    echo 'Native event translation selected an unused client decoration engine' >&2
    exit 1
fi
