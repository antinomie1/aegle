//! Bounded glyph, image and path-mask pages. Renderer fences previous GPU use
//! before each frame.
use aegle_glyph::{
    CacheLimits, CacheStats, Content, FontData, Glyph, GlyphCache, GlyphKey, Placement,
    RasterOptions,
};
use ash::vk;

pub(crate) use crate::atlas_pages::{AtlasGlyph, ResourceKey};
use crate::{
    Error, Result,
    atlas_pages::{EntryKey, Storage},
    device::Device,
    text_pipeline::TextPipeline,
    upload::Uploads,
};

/// Independent bounds for the optional GPU glyph cache and its CPU source cache.
#[derive(Clone, Copy, Debug)]
pub struct TextOptions {
    /// Square page extent, including one transparent pixel around every glyph.
    pub page_size: u32,
    /// Maximum total mask and color pages. Pages allocate only when needed.
    pub max_pages: u32,
    /// Maximum resident glyph records, each owning at most 64 variation coordinates.
    pub max_entries: u32,
    /// Maximum CPU upload vector capacity, including four-byte alignment padding.
    pub upload_bytes: usize,
    /// Independent CPU glyph image, decoder and entry limits.
    pub glyph_cache: CacheLimits,
}
impl Default for TextOptions {
    fn default() -> Self {
        Self {
            page_size: 512,
            max_pages: 8,
            max_entries: 4096,
            upload_bytes: 1024 * 1024,
            glyph_cache: CacheLimits::default(),
        }
    }
}

/// Explicit text storage; excludes font bytes, allocator and Vulkan driver metadata.
#[derive(Clone, Copy, Debug, Default)]
pub struct TextStats {
    /// Allocated image pages, across both formats.
    pub atlas_pages: u32,
    /// Glyph records in those pages, including this frame's pending inserts.
    pub atlas_entries: usize,
    /// Actual VkDeviceMemory bytes backing atlas images.
    pub atlas_bytes: u64,
    /// Actual VkDeviceMemory bytes backing the current upload buffer.
    pub staging_bytes: u64,
    /// Retained CPU upload vector capacity, bounded by TextOptions::upload_bytes.
    pub upload_bytes: usize,
    /// Pending glyph upload regions. Successful submission clears this count.
    pub upload_regions: usize,
    /// Retained region slots, at most TextOptions::max_entries.
    pub upload_region_capacity: usize,
    /// Hash table entry capacity, including spare slots from bounded table growth.
    pub entry_capacity: usize,
    /// Page slots, at most TextOptions::max_pages.
    pub page_capacity: usize,
    /// CPU cache occupancy, with its independent resource accounting.
    pub glyph_cache: CacheStats,
    /// Calls to the CPU raster cache since creation or clear, including CPU hits.
    pub raster_requests: u64,
}

pub(crate) struct Atlas {
    storage: Storage,
    uploads: Uploads,
    cache: GlyphCache,
    options: TextOptions,
    raster_requests: u64,
}
impl Atlas {
    pub fn new(options: TextOptions) -> Result<Self> {
        if options.page_size < 2
            || options.page_size > i32::MAX as u32
            || options.max_pages == 0
            || options.max_entries == 0
        {
            return Err(Error::InvalidTextOptions);
        }
        Ok(Self {
            storage: Storage::new(),
            uploads: Uploads::default(),
            cache: GlyphCache::with_limits(options.glyph_cache),
            options,
            raster_requests: 0,
        })
    }

    /// Called only after the preceding submission's fence. A cancelled frame may
    /// have extended an existing page, so rollback invalidates that entire page.
    pub fn begin_frame(&mut self) {
        for index in 0..self.storage.pages.len() {
            if self.storage.pages[index]
                .as_ref()
                .is_some_and(|page| page.dirty)
            {
                self.storage.reset(index);
            }
            if let Some(page) = self.storage.pages[index].as_mut() {
                page.pinned = false;
            }
        }
        self.uploads.begin_frame();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn get(
        &mut self,
        font: &FontData,
        id: u16,
        options: RasterOptions<'_>,
        device: &Device,
        pipeline: &TextPipeline,
        budget: u64,
        visible: impl FnOnce(Placement) -> Result<bool>,
    ) -> Result<Option<AtlasGlyph>> {
        let key = GlyphKey::new(font, id, options)?;
        if let Some(glyph) = self.storage.lookup(key) {
            if !visible(glyph.placement)? {
                return Ok(None);
            }
            self.storage.touch(glyph.page);
            return Ok(Some(glyph));
        }
        self.raster_requests = self.raster_requests.saturating_add(1);
        let glyph = self.cache.rasterize(font, id, options)?;
        if glyph.placement.width == 0 || glyph.placement.height == 0 || !visible(glyph.placement)? {
            return Ok(None);
        }
        let key = if glyph.content == Content::Mask {
            key.mask()
        } else {
            key
        };
        let hash = self.storage.hash(key);
        let budget = budget - self.uploads.device_bytes();
        self.storage
            .insert(
                EntryKey::Glyph(key.to_owned()),
                hash,
                glyph,
                &mut self.uploads,
                self.options,
                device,
                pipeline,
                budget,
            )
            .map(Some)
    }

    /// Returns a cached non-glyph entry and pins its page for this frame.
    pub fn resource(&mut self, key: ResourceKey) -> Option<AtlasGlyph> {
        let entry = self.storage.lookup_resource(key)?;
        self.storage.touch(entry.page);
        Some(entry)
    }

    /// Uploads image or path-mask pixels missing from [`Self::resource`].
    pub fn insert_resource(
        &mut self,
        key: ResourceKey,
        glyph: Glyph<'_>,
        device: &Device,
        pipeline: &TextPipeline,
        budget: u64,
    ) -> Result<AtlasGlyph> {
        let hash = self.storage.hash(key);
        let budget = budget - self.uploads.device_bytes();
        self.storage.insert(
            EntryKey::Resource(key),
            hash,
            glyph,
            &mut self.uploads,
            self.options,
            device,
            pipeline,
            budget,
        )
    }

    pub fn prepare_upload(&mut self, device: &Device, budget: u64) -> Result {
        self.uploads.prepare(device, budget - self.storage.bytes())
    }

    /// Command buffer is recording outside a render pass; prepare_upload succeeded.
    /// Renderer keeps all pages and staging alive until the submission fence.
    pub fn record_uploads(&self, raw: &ash::Device, command: vk::CommandBuffer) {
        for (index, page) in self.storage.pages.iter().enumerate() {
            if let Some(page) = page.as_ref().filter(|page| page.dirty) {
                self.uploads.record(
                    raw,
                    command,
                    index as u32,
                    page.image.handle,
                    page.initialized,
                    page.reset,
                );
            }
        }
    }

    /// Called only after queue_submit succeeds; layouts and entries then become
    /// committed together. Device loss is reported by Renderer when waiting.
    pub fn commit(&mut self) {
        for page in self.storage.pages.iter_mut().flatten().filter(|p| p.dirty) {
            page.initialized = true;
            page.reset = false;
            page.dirty = false;
        }
        self.uploads.commit();
    }

    /// Releases transfer memory after the submission fence, retaining CPU records.
    pub fn retire_upload(&mut self) {
        self.uploads.retire();
    }

    pub fn device_bytes(&self) -> u64 {
        self.storage.bytes() + self.uploads.device_bytes()
    }

    pub fn stats(&self) -> TextStats {
        TextStats {
            atlas_pages: self.storage.pages.iter().flatten().count() as u32,
            atlas_entries: self.storage.entries(),
            atlas_bytes: self.storage.bytes(),
            staging_bytes: self.uploads.device_bytes(),
            upload_bytes: self.uploads.bytes.capacity(),
            upload_regions: self.uploads.regions.len(),
            upload_region_capacity: self.uploads.regions.capacity(),
            entry_capacity: self.storage.entry_capacity(),
            page_capacity: self.storage.pages.capacity(),
            glyph_cache: self.cache.stats(),
            raster_requests: self.raster_requests,
        }
    }

    /// Renderer has waited before dropping referenced images or upload memory.
    pub fn clear(&mut self) {
        self.storage = Storage::new();
        self.uploads = Uploads::default();
        self.cache.clear();
        self.raster_requests = 0;
    }
}
