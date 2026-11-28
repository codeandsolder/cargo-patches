// Copyright 2026 the Vello Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Persistent renderer resources shared across frames.

#[cfg(feature = "text")]
use crate::text::GlyphAtlasResources;
#[cfg(feature = "text")]
use glifo::GlyphPrepCache;
use vello_common::image_cache::ImageCache;
use vello_common::multi_atlas::AtlasConfig;

/// Persistent resources required by Vello GPU for rendering.
///
/// A set of resources must only be used with the renderer instance associated with it.
#[derive(Debug)]
pub struct Resources {
    pub(crate) image_cache: ImageCache,
    #[cfg(feature = "text")]
    pub(crate) glyph_prep_cache: GlyphPrepCache,
    #[cfg(feature = "text")]
    pub(crate) glyph_resources: Option<GlyphAtlasResources>,
}

/// Current glyph atlas residency and cumulative cache activity.
#[cfg(feature = "text")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphAtlasStats {
    /// Resident glyph entries.
    pub entries: usize,
    /// Successful lookups.
    pub hits: u64,
    /// Unsuccessful lookups.
    pub misses: u64,
    /// Allocated texture pages.
    pub pages: usize,
}

impl Resources {
    /// Glyph atlas activity, or None before the first cached glyph.
    #[cfg(feature = "text")]
    pub fn glyph_atlas_stats(&self) -> Option<GlyphAtlasStats> {
        let atlas = &self.glyph_resources.as_ref()?.glyph_atlas;
        Some(GlyphAtlasStats {
            entries: atlas.len(),
            hits: atlas.cache_hits(),
            misses: atlas.cache_misses(),
            pages: self.image_cache.atlas_count(),
        })
    }

    pub(crate) fn new(glyph_atlas_config: AtlasConfig) -> Self {
        Self {
            image_cache: ImageCache::new_with_config(glyph_atlas_config),
            #[cfg(feature = "text")]
            glyph_prep_cache: GlyphPrepCache::default(),
            // Will be initialized lazily.
            #[cfg(feature = "text")]
            glyph_resources: None,
        }
    }

    /// Drop all cached glyphs and their atlas allocations.
    pub(crate) fn clear(&mut self) {
        *self = Self::new(*self.image_cache.atlas_manager().config());
    }
}
