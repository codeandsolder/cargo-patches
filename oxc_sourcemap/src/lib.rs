mod compose;
mod concat_sourcemap_builder;
mod decode;
mod encode;
mod error;
mod lookup_cursor;
mod owned_sourcemap;
mod sourcemap;
mod sourcemap_builder;
mod sourcemap_visualizer;
mod token;

#[cfg(feature = "napi")]
pub mod napi;

pub use concat_sourcemap_builder::ConcatSourceMapBuilder;
pub use decode::JSONSourceMap;
pub use error::Error;
pub use lookup_cursor::LookupCursor;
pub use owned_sourcemap::OwnedSourceMap;
pub use sourcemap::{SourceMap, SourceMapParts};
pub use sourcemap_builder::SourceMapBuilder;
pub use sourcemap_visualizer::SourcemapVisualizer;
pub use token::{SourceViewToken, Token, TokenChunk};

fn encode_base64(input: &[u8], prefix: &str) -> String {
    use base64::Engine;
    let encoded_len = base64::encoded_len(input.len(), true).expect("base64 length overflow");
    let capacity = prefix.len().checked_add(encoded_len).expect("base64 URL length overflow");
    let mut output = String::with_capacity(capacity);
    output.push_str(prefix);
    #[cfg(any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "wasm32", target_feature = "simd128")
    ))]
    {
        static ENGINE: std::sync::LazyLock<base64::engine::Simd> = std::sync::LazyLock::new(|| {
            base64::engine::Simd::standard(base64::engine::general_purpose::PAD)
        });
        ENGINE.encode_string(input, &mut output);
    }
    #[cfg(not(any(
        target_arch = "x86_64",
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "wasm32", target_feature = "simd128")
    )))]
    {
        base64::engine::general_purpose::STANDARD.encode_string(input, &mut output);
    }
    output
}
