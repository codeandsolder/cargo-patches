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

fn encode_base64(input: &[u8]) -> String {
    use base64::Engine;
    #[cfg(any(target_arch = "x86_64", all(target_arch = "aarch64", target_feature = "neon")))]
    {
        static ENGINE: std::sync::LazyLock<base64::engine::Simd> =
            std::sync::LazyLock::new(|| base64::engine::Simd::standard(base64::engine::general_purpose::PAD));
        ENGINE.encode(input)
    }
    #[cfg(not(any(target_arch = "x86_64", all(target_arch = "aarch64", target_feature = "neon"))))]
    {
        base64::engine::general_purpose::STANDARD.encode(input)
    }
}
