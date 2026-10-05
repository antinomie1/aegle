use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use hashbrown::HashTable;
use lru_slab::LruSlab;

use crate::raster::Rasterizer;
use crate::{
    CacheLimits, CacheStats, Content, FontData, Glyph, GlyphError, GlyphKey, Image, OwnedGlyphKey,
    RasterOptions,
};

struct Entry {
    key: OwnedGlyphKey,
    hash: u64,
    image: Image,
}

struct Storage {
    index: HashTable<u32>,
    entries: LruSlab<Entry>,
    bytes: usize,
}

impl Storage {
    fn new() -> Self {
        Self {
            index: HashTable::new(),
            entries: LruSlab::new(),
            bytes: 0,
        }
    }

    fn reserve(&mut self, bytes: usize, limits: CacheLimits) -> Result<(), GlyphError> {
        if bytes > limits.image_bytes || limits.entries == 0 {
            return Err(GlyphError::ImageBudget);
        }
        while self.bytes > limits.image_bytes - bytes || self.entries.len() >= limits.entries {
            let slot = self.entries.lru().expect("nonempty over-budget cache");
            let entry = self.entries.remove(slot);
            self.index
                .find_entry(entry.hash, |value| *value == slot)
                .expect("every LRU entry has an index")
                .remove();
            self.bytes -= entry.image.data.len();
        }
        Ok(())
    }
}

/// Hash-indexed LRU glyph images with independent pixel and entry limits.
///
/// Hits perform no allocation. Misses can evict older entries before rendering;
/// a raster error may therefore leave a smaller cache. No font blobs are retained.
pub struct GlyphCache {
    storage: Storage,
    hash: RandomState,
    limits: CacheLimits,
    raster: Rasterizer,
}

impl GlyphCache {
    /// Creates an empty cache. Tables and glyph pixels allocate on demand.
    pub fn with_limits(limits: CacheLimits) -> Self {
        Self {
            storage: Storage::new(),
            hash: RandomState::new(),
            limits,
            raster: Rasterizer::new(),
        }
    }

    /// Returns an existing image or rasterizes and stores the requested glyph.
    ///
    /// Use the image immediately, or retain the font/glyph request and ask again.
    /// Font data must remain immutable for its blob ID's entire lifetime.
    pub fn rasterize(
        &mut self,
        font: &FontData,
        glyph_id: u16,
        options: RasterOptions<'_>,
    ) -> Result<Glyph<'_>, GlyphError> {
        let key = GlyphKey::new(font, glyph_id, options)?;
        let mask_key = key.mask();
        let mask_hash = self.hash.hash_one(mask_key);
        let mask_slot = self
            .storage
            .index
            .find(mask_hash, |slot| {
                let entry = self.storage.entries.peek(*slot);
                entry.key.as_key() == mask_key && entry.image.content == Content::Mask
            })
            .copied();
        let slot = if let Some(slot) = mask_slot {
            slot
        } else {
            let hash = self.hash.hash_one(key);
            if let Some(&slot) = self.storage.index.find(hash, |slot| {
                let entry = self.storage.entries.peek(*slot);
                entry.key.as_key() == key
            }) {
                return Ok(self.storage.entries.get_mut(slot).image.as_glyph());
            }
            let image = self
                .raster
                .rasterize(font, glyph_id, options, self.limits, |bytes| {
                    self.storage.reserve(bytes, self.limits)
                })?;
            let (key, hash) = if image.content == Content::Mask {
                (mask_key, mask_hash)
            } else {
                (key, hash)
            };
            self.storage.bytes += image.data.len();
            let slot = self.storage.entries.insert(Entry {
                key: key.to_owned(),
                hash,
                image,
            });
            self.storage
                .index
                .insert_unique(hash, slot, |slot| self.storage.entries.peek(*slot).hash);
            slot
        };
        Ok(self.storage.entries.get_mut(slot).image.as_glyph())
    }

    /// Returns exact pixel occupancy and table capacities.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            image_bytes: self.storage.bytes,
            entries: self.storage.entries.len(),
            entry_capacity: self.storage.entries.capacity(),
            index_capacity: self.storage.index.capacity(),
        }
    }

    /// Releases Swash, outline and raster scratch, keeping completed glyphs.
    ///
    /// Swash keeps at most four font proxies here and up to eight hint instances
    /// per outline format internally. It exposes no byte accounting for them.
    pub fn release_scratch(&mut self) {
        self.raster = Rasterizer::new();
    }

    /// Releases glyphs, key storage, indices and all scaling scratch.
    pub fn clear(&mut self) {
        self.storage = Storage::new();
        self.release_scratch();
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::with_limits(CacheLimits::default())
    }
}
