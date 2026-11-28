// Copyright 2026 the Imaging Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! GPU regression for the immediate-frame glyph atlas.

use imaging::{Painter, record::Scene, render::ImageRenderer};
use imaging_vello_hybrid::{VelloHybridRenderer, wgpu};
use peniko::{Blob, Color};
use std::sync::Arc;

#[test]
fn immediate_glyph_atlas_reuses_entries_and_preserves_geometry() {
    let (device, queue) = pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("GPU required for atlas validation");
        adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap()
    });
    let mut renderer = VelloHybridRenderer::new(device, queue);
    let font = peniko::FontData::new(
        Blob::new(Arc::new(include_bytes!(
            "../../test_assets/fonts/NotoSans-Regular.ttf"
        ))),
        0,
    );
    let make_scene = |id, color, transform| {
        let mut scene = Scene::new();
        Painter::new(&mut scene)
            .glyphs(&font, color)
            .font_size(24.0)
            .transform(transform)
            .draw(
                &peniko::Style::Fill(peniko::Fill::NonZero),
                [imaging::record::Glyph { id, x: 0.0, y: 0.0 }],
            );
        scene
    };
    let origin = kurbo::Affine::translate((16.0, 40.0));
    let scene = make_scene(36, Color::BLACK, origin);
    let retained = renderer.encode_scene(&scene, 64, 64).unwrap();
    assert!(renderer.glyph_atlas_stats().is_none());
    let first = ImageRenderer::render_source(&mut renderer, &mut &scene, 64, 64).unwrap();
    assert!(first.data.chunks_exact(4).any(|p| p[3] != 0));
    let initial = renderer.glyph_atlas_stats().unwrap();
    assert_eq!(initial.entries, 1);
    assert_eq!(initial.pages, 1);
    let again = ImageRenderer::render_source(&mut renderer, &mut &scene, 64, 64).unwrap();
    assert_eq!(first.data, again.data);
    let warm = renderer.glyph_atlas_stats().unwrap();
    assert_eq!(warm.entries, 1);
    assert!(warm.hits > initial.hits);

    let red = make_scene(36, Color::from_rgb8(255, 0, 0), origin);
    let tinted = ImageRenderer::render_source(&mut renderer, &mut &red, 64, 64).unwrap();
    for (a, b) in first.data.chunks_exact(4).zip(tinted.data.chunks_exact(4)) {
        assert_eq!(a[3], b[3]);
        if b[3] > 0 {
            assert_eq!(b[0], 255);
            assert_eq!(b[1], 0);
            assert_eq!(b[2], 0);
        }
    }
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 1);

    let shifted = make_scene(36, Color::BLACK, kurbo::Affine::translate((16.0, 40.5)));
    let shifted = ImageRenderer::render_source(&mut renderer, &mut &shifted, 64, 64).unwrap();
    assert_ne!(first.data, shifted.data);
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 2);
    let near_integer = make_scene(36, Color::BLACK, kurbo::Affine::translate((15.99, 39.99)));
    let near_integer =
        ImageRenderer::render_source(&mut renderer, &mut &near_integer, 64, 64).unwrap();
    assert_eq!(first.data, near_integer.data);
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 2);

    let rotated = make_scene(
        36,
        Color::BLACK,
        kurbo::Affine::rotate(0.35).then_translate((16.0, 40.0).into()),
    );
    let uncached = renderer.encode_scene(&rotated, 64, 64).unwrap();
    let reference = renderer.render(&uncached, 64, 64).unwrap();
    let actual = ImageRenderer::render_source(&mut renderer, &mut &rotated, 64, 64).unwrap();
    assert_eq!(
        reference.data, actual.data,
        "warm atlas must not change rotated glyphs"
    );

    let other = make_scene(37, Color::BLACK, origin);
    let other_pixels = ImageRenderer::render_source(&mut renderer, &mut &other, 64, 64).unwrap();
    assert_ne!(first.data, other_pixels.data);
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 3);

    // A public retained scene contains no evictable atlas references.
    let leased = renderer
        .prepare_outline_glyphs(&font, 24.0, false, &[36])
        .unwrap();
    let slot = leased.slots().next().unwrap();
    assert!(slot.width > 0 && slot.height > 0);
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 3);
    assert_eq!(leased.pages().len(), 1);
    let before = renderer.render(&retained, 64, 64).unwrap();
    for _ in 0..140 {
        ImageRenderer::render_source(&mut renderer, &mut &other, 64, 64).unwrap();
    }
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 2);
    drop(leased);
    for _ in 0..140 {
        ImageRenderer::render_source(&mut renderer, &mut &other, 64, 64).unwrap();
    }
    assert_eq!(renderer.glyph_atlas_stats().unwrap().entries, 1);
    let after = renderer.render(&retained, 64, 64).unwrap();
    assert_eq!(before.data, after.data);
}
