#!/usr/bin/env bash
set -euo pipefail

# This source is not published with the upstream packages.
cat > image_features.rs <<'RS'
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
#[cfg(feature = "encoded-formats")]
use image::{ImageEncoder, codecs::png::PngEncoder};
use oar_ocr::{processors::BoundingBox, utils::{get_rotate_crop_image, image::load_image_from_memory}};
use std::io::Cursor;

#[cfg(feature = "encoded-formats")]
#[test]
fn png_and_pnm_preserve_rgb_and_crop_geometry() {
    let source = RgbImage::from_fn(8, 6, |x, y| Rgb([x as u8 * 19, y as u8 * 23, 91]));
    let mut png = Vec::new();
    PngEncoder::new(&mut png).write_image(source.as_raw(), 8, 6, image::ExtendedColorType::Rgb8).unwrap();
    let mut pnm = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(source.clone()).write_to(&mut pnm, ImageFormat::Pnm).unwrap();
    let bounds = BoundingBox::from_coords(1.0, 1.0, 7.0, 5.0);
    let expected = get_rotate_crop_image(&source, &bounds.points).unwrap();
    for bytes in [&png, pnm.get_ref()] {
        let decoded = load_image_from_memory(bytes).unwrap();
        assert_eq!(decoded, source);
        assert_eq!(get_rotate_crop_image(&decoded, &bounds.points).unwrap(), expected);
    }
}

#[test]
fn raw_rgb_crop_needs_no_codecs() {
    let source = RgbImage::from_pixel(8, 6, Rgb([7, 42, 91]));
    let bounds = BoundingBox::from_coords(1.0, 1.0, 7.0, 5.0);
    let crop = get_rotate_crop_image(&source, &bounds.points).unwrap();
    assert_eq!(crop.dimensions(), (6, 4));
    assert!(crop.pixels().all(|pixel| *pixel == Rgb([7, 42, 91])));
}

#[test]
fn format_support_is_selected_by_consumer() {
    let source = RgbImage::from_pixel(2, 2, Rgb([7, 42, 91]));
    let mut bytes = Cursor::new(Vec::new());
    let encoded = DynamicImage::ImageRgb8(source.clone()).write_to(&mut bytes, ImageFormat::Bmp);
    #[cfg(feature = "full-formats")]
    {
        encoded.unwrap();
        assert_eq!(load_image_from_memory(bytes.get_ref()).unwrap(), source);
    }
    #[cfg(not(feature = "full-formats"))]
    assert!(encoded.is_err());
}
RS

raw=$(cargo tree --locked --no-default-features -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$raw" | rg '^(png|fdeflate|miniz_oxide|zune-jpeg) v'; then
    echo 'Raw RGB configuration must not compile image codecs or Miniz' >&2
    exit 1
fi
printf '%s\n' "$raw" | rg '^zlib-rs v' >/dev/null
narrow=$(cargo tree --locked -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$narrow" | rg '^(rav1e|ravif|exr|gif|tiff|zune-jpeg|ab_glyph|owned_ttf_parser|ttf-parser|rustdct) v'; then
    echo 'Unused image codec or imageproc subsystem in narrow graph' >&2
    exit 1
fi
printf '%s\n' "$narrow" | rg '^png v' >/dev/null
printf '%s\n' "$narrow" | rg '^rayon v' >/dev/null
full=$(cargo tree --locked --features full-formats -e normal,build --prefix none --format '{p}')
for crate in ravif exr gif tiff zune-jpeg; do
    printf '%s\n' "$full" | rg "^$crate v" >/dev/null
done
if printf '%s\n' "$full" | rg '^(ab_glyph|owned_ttf_parser|ttf-parser|rustdct) v'; then
    echo 'Unused imageproc text/FFT subsystem in full graph' >&2
    exit 1
fi
cargo test --locked --no-default-features --test image_features
cargo test --locked --test image_features
cargo test --locked --features full-formats --test image_features
