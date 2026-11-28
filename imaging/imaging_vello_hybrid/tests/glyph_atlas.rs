// Copyright 2026 the Imaging Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! GPU regression for the immediate-frame glyph atlas.

use imaging::{Painter, record::Scene, render::ImageRenderer};
use imaging_vello_hybrid::{OutlineGlyphStyle, VelloHybridRenderer, wgpu};
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
        .prepare_outline_glyphs(
            &font,
            OutlineGlyphStyle {
                font_size: 24.0,
                hint: false,
                normalized_coords: &[],
            },
            &[36],
        )
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

#[test]
fn color_and_bitmap_glyphs_preserve_color_coverage_and_opacity() {
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
    for (name, bytes) in [
        (
            "COLR",
            include_bytes!("../../test_assets/fonts/AtlasCOLR.ttf").as_slice(),
        ),
        (
            "PNG",
            include_bytes!("../../test_assets/fonts/AtlasPNG.ttf").as_slice(),
        ),
        (
            "BGRA",
            include_bytes!("../../test_assets/fonts/AtlasBGRA.ttf").as_slice(),
        ),
        (
            "mask",
            include_bytes!("../../test_assets/fonts/AtlasMask.ttf").as_slice(),
        ),
    ] {
        let font = peniko::FontData::new(Blob::new(Arc::new(bytes)), 0);
        let make_scene = |alpha, brush: &peniko::Brush| {
            let mut scene = Scene::new();
            Painter::new(&mut scene)
                .glyphs(&font, brush)
                .font_size(16.0)
                .transform(kurbo::Affine::translate((8.0, 40.0)))
                .composite(imaging::Composite {
                    alpha,
                    ..Default::default()
                })
                .draw(
                    &peniko::Style::Fill(peniko::Fill::NonZero),
                    [imaging::record::Glyph {
                        id: 1,
                        x: 0.0,
                        y: 0.0,
                    }],
                );
            scene
        };
        let blue = peniko::Brush::Solid(Color::from_rgb8(0, 0, 255));
        let full = make_scene(1.0, &blue);
        let image = ImageRenderer::render_source(&mut renderer, &mut &full, 64, 64)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let pixel = |x: usize, y: usize| &image.data[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        let left = pixel(9, 32);
        let right = pixel(22, 32);
        if name == "mask" {
            assert!(left[2] > 240 && left[3] > 240, "{name}: {left:?}");
            assert!(right[2] > 240 && right[3] > 240, "{name}: {right:?}");
        } else {
            assert!(left[0] > 240 && left[3] > 240, "{name}: {left:?}");
            assert!(right[1] > 240 && right[3] > 100, "{name}: {right:?}");
        }
        let before = renderer.glyph_atlas_stats().unwrap();
        let half = make_scene(0.5, &blue);
        let faded = ImageRenderer::render_source(&mut renderer, &mut &half, 64, 64).unwrap();
        for (a, b) in image.data.chunks_exact(4).zip(faded.data.chunks_exact(4)) {
            assert!(
                u16::from(a[3]).abs_diff(2 * u16::from(b[3])) <= 2,
                "{name}: {a:?} -> {b:?}"
            );
        }
        let after = renderer.glyph_atlas_stats().unwrap();
        assert_eq!(
            before.entries, after.entries,
            "opacity is not atlas content: {name}"
        );
        assert!(after.hits > before.hits, "{name}");
        let retained = renderer.encode_scene(&half, 64, 64).unwrap();
        let direct = renderer.render(&retained, 64, 64).unwrap();
        for (index, (cached, direct)) in faded
            .data
            .chunks_exact(4)
            .zip(direct.data.chunks_exact(4))
            .enumerate()
        {
            // Compare premultiplied values: tiny rounding near zero alpha can
            // amplify harmless quantization when converted to straight RGBA.
            let premul = |pixel: &[u8], channel: usize| {
                (u16::from(pixel[channel]) * u16::from(pixel[3]) + 127) / 255
            };
            assert!(
                cached[3].abs_diff(direct[3]) <= 2
                    && (0..3).all(|c| premul(cached, c).abs_diff(premul(direct, c)) <= 2),
                "{name}: retained vs cached at ({}, {}): {cached:?} -> {direct:?}",
                index % 64,
                index / 64
            );
        }
        if name == "mask" {
            let gradient = peniko::Brush::Gradient(
                peniko::Gradient::new_linear((0.0, 0.0), (16.0, 0.0))
                    .with_stops([Color::from_rgb8(255, 0, 0), Color::from_rgb8(0, 255, 0)]),
            );
            let scene = make_scene(1.0, &gradient);
            let colored = ImageRenderer::render_source(&mut renderer, &mut &scene, 64, 64).unwrap();
            let left = &colored.data[(32 * 64 + 9) * 4..(32 * 64 + 9) * 4 + 4];
            let right = &colored.data[(32 * 64 + 22) * 4..(32 * 64 + 22) * 4 + 4];
            assert!(
                left[0] > left[1] && left[3] > 240,
                "gradient mask: {left:?}"
            );
            assert!(
                right[1] > right[0] && right[3] > 240,
                "gradient mask: {right:?}"
            );
            assert!(
                colored.data[(32 * 64 + 18) * 4 + 3] < 64,
                "mask hole must remain transparent"
            );
        }
    }
}

#[test]
fn prewarm_matches_variable_instances_and_canonical_defaults() {
    let (device, queue) = pollster::block_on(async {
        let adapter = wgpu::Instance::default()
            .request_adapter(&Default::default())
            .await
            .expect("GPU required for atlas validation");
        adapter.request_device(&Default::default()).await.unwrap()
    });
    let mut renderer = VelloHybridRenderer::new(device, queue);
    let font = peniko::FontData::new(
        Blob::new(Arc::new(include_bytes!(
            "../../test_assets/fonts/AtlasVariable.ttf"
        ))),
        0,
    );
    let mut scene = Scene::new();
    Painter::new(&mut scene)
        .glyphs(&font, Color::BLACK)
        .font_size(32.0)
        .normalized_coords(&[0])
        .hint(false)
        .transform(kurbo::Affine::translate((8.0, 40.0)))
        .draw(
            &peniko::Style::Fill(peniko::Fill::NonZero),
            [imaging::record::Glyph {
                id: 1,
                x: 0.0,
                y: 0.0,
            }],
        );
    ImageRenderer::render_source(&mut renderer, &mut &scene, 64, 64).unwrap();
    let before = renderer.glyph_atlas_stats().unwrap();
    let normal = renderer
        .prepare_outline_glyphs(
            &font,
            OutlineGlyphStyle {
                font_size: 32.0,
                hint: false,
                normalized_coords: &[],
            },
            &[1],
        )
        .unwrap();
    let after = renderer.glyph_atlas_stats().unwrap();
    assert_eq!(before.entries, after.entries);
    assert!(after.hits > before.hits);
    let bold = renderer
        .prepare_outline_glyphs(
            &font,
            OutlineGlyphStyle {
                font_size: 32.0,
                hint: false,
                normalized_coords: &[16384],
            },
            &[1],
        )
        .unwrap();
    assert!(bold.slots().next().unwrap().width > normal.slots().next().unwrap().width);
    assert_eq!(
        renderer.glyph_atlas_stats().unwrap().entries,
        after.entries + 1
    );
}
