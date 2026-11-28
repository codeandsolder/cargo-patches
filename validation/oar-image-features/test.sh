#!/usr/bin/env bash
set -euo pipefail

# This source is not published with the upstream packages.
cat > image_features.rs <<'RS'
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
#[cfg(feature = "encoded-formats")]
use image::{ImageEncoder, codecs::png::PngEncoder};
use oar_ocr::{processors::BoundingBox, utils::get_rotate_crop_image};
#[cfg(any(feature = "encoded-formats", feature = "full-formats"))]
use oar_ocr::utils::image::load_image_from_memory;
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
#[test]
fn layout_builder_remains_available_without_formula_models() {
    let _builder = oar_ocr::oarocr::OARStructureBuilder::new("layout.onnx")
        .with_table_classification("table.onnx");
    assert_eq!(oar_ocr::core::traits::TaskType::TextDetection.name(), "text_detection");
}

#[cfg(feature = "formula-recognition")]
#[test]
fn formula_api_is_available_when_selected() {
    let _builder = oar_ocr::oarocr::OARStructureBuilder::new("layout.onnx")
        .with_formula_recognition("formula.onnx", "tokenizer.json", "pp_formulanet");
    assert_eq!(oar_ocr::core::traits::TaskType::FormulaRecognition.name(), "formula_recognition");
}

#[cfg(feature = "indra")]
#[test]
fn indra_api_is_available_when_selected() {
    let _builder = oar_ocr::indra::IndraOcrBuilder::new("models");
}
RS

raw=$(cargo tree --locked --no-default-features -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$raw" | rg '^(png|fdeflate|miniz_oxide|zune-jpeg|flate2|zip|tokenizers|onig|onig_sys|paste|unicode-bidi) v'; then
    echo 'Raw RGB configuration must not compile image codecs or Miniz' >&2
    exit 1
fi
narrow=$(cargo tree --locked -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$narrow" | rg '^(rav1e|ravif|exr|gif|tiff|zune-jpeg|ab_glyph|owned_ttf_parser|ttf-parser|rustdct|zip|tokenizers|onig|onig_sys|paste|unicode-bidi) v'; then
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
formula=$(cargo tree --locked --no-default-features --features formula-recognition -e normal,build --prefix none --format '{p}')
printf '%s\n' "$formula" | rg '^tokenizers v' >/dev/null
if printf '%s\n' "$formula" | rg '^(zip|unicode-bidi) v'; then
    echo 'Formula recognition must not enable Indra' >&2
    exit 1
fi
indra=$(cargo tree --locked --no-default-features --features indra -e normal,build --prefix none --format '{p}')
for crate in zip zlib-rs unicode-bidi; do
    printf '%s\n' "$indra" | rg "^$crate v" >/dev/null
done
if printf '%s\n' "$indra" | rg '^(tokenizers|onig|onig_sys|paste|miniz_oxide) v'; then
    echo 'Indra must not enable formula recognition or Miniz' >&2
    exit 1
fi
cargo check --locked --no-default-features --features formula-recognition --tests
cargo check --locked --no-default-features --features indra --tests
cargo test --locked --features full-formats,formula-recognition,indra --test image_features

# Exercise the actual downloader helpers without fetching external models or
# bypassing the compiler cache. The build script itself compiles with
# download-binaries enabled; load-dynamic avoids linking the distribution.
cat > download_tls.rs <<'RS'
#[path = "ort-sys/build/download/tls.rs"]
mod tls;
#[path = "ort-sys/build/download/verify.rs"]
mod verify;
use std::io::{Cursor, Read};
use ureq::tls::{RootCerts, TlsProvider};

#[test]
fn rustls_retains_platform_trust_and_verification() {
    let config = tls::download_tls_config();
    assert_eq!(config.provider(), TlsProvider::Rustls);
    assert!(matches!(config.root_certs(), RootCerts::PlatformVerifier));
    assert!(!config.disable_verification());
}

#[test]
fn sha256_covers_partially_read_and_unread_data() {
    let mut reader = verify::VerifyReader::new(Cursor::new(b"abc"));
    let mut prefix = [0; 1];
    reader.read_exact(&mut prefix).unwrap();
    assert_eq!(prefix, [b'a']);
    let (digest, source) = reader.finalize().unwrap();
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert_eq!(verify::bytes_to_hex_str(&digest), expected);
    assert_eq!(digest.as_slice(), verify::hex_str_to_bytes(expected));
    assert_eq!(source.position(), 3);
}
RS
download=$(cargo tree --locked --target all --no-default-features --features tls-rustls-platform -e normal,build --prefix none --format '{p}')
if printf '%s\n' "$download" | rg '^(native-tls|openssl|openssl-sys|hmac-sha256) v|^sha2 v0\.10'; then
    echo 'Unexpected native TLS or duplicate SHA-256 implementation' >&2
    exit 1
fi
for crate in rustls rustls-platform-verifier ureq sha2; do
    printf '%s\n' "$download" | rg "^$crate v" >/dev/null
done
cargo test --locked --no-default-features --features tls-rustls-platform --test download_tls
