//! Bounded glyph pages. Renderer fences previous GPU use before each frame.
use std::{collections::hash_map::RandomState, hash::BuildHasher};

use aegle_glyph::{
    CacheLimits, CacheStats, Content, FontData, Glyph, GlyphCache, GlyphKey, OwnedGlyphKey,
    Placement, RasterOptions,
};
use ash::vk;
use hashbrown::HashTable;

use crate::{
    Error, Result, device::Device, memory::Image, text_pipeline::TextPipeline, upload::Uploads,
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

#[derive(Clone, Copy)]
pub(crate) struct AtlasGlyph {
    pub placement: Placement,
    pub page: u32,
    pub rect: [f32; 4],
    pub content: Content,
}

struct Entry {
    key: OwnedGlyphKey,
    hash: u64,
    glyph: AtlasGlyph,
}

#[derive(Clone, Copy, Default)]
struct Shelf {
    x: u32,
    y: u32,
    height: u32,
}
impl Shelf {
    fn place(mut self, width: u32, height: u32, side: u32) -> Option<(Self, [u32; 2])> {
        if self.x + width > side {
            self.x = 0;
            self.y += self.height;
            self.height = 0;
        }
        if self.y + height > side {
            return None;
        }
        let position = [self.x + 1, self.y + 1];
        self.x += width;
        self.height = self.height.max(height);
        Some((self, position))
    }
}

struct Page {
    image: Image,
    content: Content,
    shelf: Shelf,
    used: u64,
    entries: u32,
    pinned: bool,
    initialized: bool,
    reset: bool,
    dirty: bool,
}

struct Storage {
    pages: Vec<Option<Page>>,
    entries: HashTable<Entry>,
    hash: RandomState,
    clock: u64,
}
impl Storage {
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            entries: HashTable::new(),
            hash: RandomState::new(),
            clock: 0,
        }
    }

    fn bytes(&self) -> u64 {
        self.pages
            .iter()
            .flatten()
            .map(|p| p.image.allocation)
            .sum()
    }

    fn touch(&mut self, index: u32) {
        self.clock = self.clock.wrapping_add(1);
        let page = self.pages[index as usize].as_mut().unwrap();
        page.used = self.clock;
        page.pinned = true;
    }

    fn lookup(&self, key: GlyphKey<'_>) -> Option<AtlasGlyph> {
        let mask = key.mask();
        self.entries
            .find(self.hash.hash_one(mask), |entry| {
                entry.glyph.content == Content::Mask && entry.key.as_key() == mask
            })
            .or_else(|| {
                self.entries
                    .find(self.hash.hash_one(key), |entry| entry.key.as_key() == key)
            })
            .map(|entry| entry.glyph)
    }

    fn oldest(&self, with_entries: bool) -> Option<usize> {
        self.pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| {
                page.as_ref()
                    .filter(|page| !page.pinned && (!with_entries || page.entries != 0))
                    .map(|page| (index, self.clock.wrapping_sub(page.used)))
            })
            .max_by_key(|(_, age)| *age)
            .map(|(index, _)| index)
    }

    fn reset(&mut self, index: usize) {
        self.entries
            .retain(|entry| entry.glyph.page as usize != index);
        let page = self.pages[index].as_mut().unwrap();
        page.shelf = Shelf::default();
        page.entries = 0;
        page.reset = true;
        page.dirty = false;
    }

    #[allow(clippy::too_many_arguments)]
    fn page(
        &mut self,
        content: Content,
        extent: [u32; 2],
        options: TextOptions,
        device: &Device,
        pipeline: &TextPipeline,
        budget: u64,
    ) -> Result<usize> {
        for (index, page) in self.pages.iter().enumerate() {
            if page.as_ref().is_some_and(|p| {
                p.content == content
                    && p.shelf
                        .place(extent[0], extent[1], options.page_size)
                        .is_some()
            }) {
                return Ok(index);
            }
        }
        loop {
            let vacant = self.pages.iter().position(Option::is_none).or_else(|| {
                (self.pages.len() < options.max_pages as usize).then_some(self.pages.len())
            });
            let mut allocation_error = None;
            if let Some(index) = vacant {
                if index == self.pages.len() {
                    self.pages
                        .try_reserve_exact(1)
                        .map_err(|_| Error::Allocation)?;
                }
                let format = match content {
                    Content::Mask => vk::Format::R8_UNORM,
                    Content::Color => vk::Format::R8G8B8A8_SRGB,
                };
                match Image::new(
                    device,
                    options.page_size,
                    options.page_size,
                    format,
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    budget - self.bytes(),
                ) {
                    Ok(image) => {
                        pipeline.update(index as u32, image.view);
                        let page = Some(Page {
                            image,
                            content,
                            shelf: Shelf::default(),
                            used: 0,
                            entries: 0,
                            pinned: false,
                            initialized: false,
                            reset: true,
                            dirty: false,
                        });
                        if index == self.pages.len() {
                            self.pages.push(page);
                        } else {
                            self.pages[index] = page;
                        }
                        return Ok(index);
                    }
                    Err(error @ Error::Budget { .. }) => allocation_error = Some(error),
                    Err(error) => return Err(error),
                }
            }
            let index = self
                .oldest(false)
                .ok_or_else(|| allocation_error.unwrap_or(Error::AtlasFull))?;
            self.reset(index);
            if self.pages[index].as_ref().unwrap().content == content {
                return Ok(index);
            }
            self.pages[index] = None;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        key: GlyphKey<'_>,
        glyph: Glyph<'_>,
        uploads: &mut Uploads,
        options: TextOptions,
        device: &Device,
        pipeline: &TextPipeline,
        budget: u64,
    ) -> Result<AtlasGlyph> {
        let width = glyph
            .placement
            .width
            .checked_add(2)
            .ok_or(Error::GlyphTooLarge)?;
        let height = glyph
            .placement
            .height
            .checked_add(2)
            .ok_or(Error::GlyphTooLarge)?;
        if width > options.page_size || height > options.page_size {
            return Err(Error::GlyphTooLarge);
        }
        while self.entries.len() >= options.max_entries as usize {
            let index = self.oldest(true).ok_or(Error::AtlasFull)?;
            self.reset(index);
        }
        self.entries
            .try_reserve(1, |entry| entry.hash)
            .map_err(|_| Error::Allocation)?;
        let index = self.page(
            glyph.content,
            [width, height],
            options,
            device,
            pipeline,
            budget,
        )?;
        let page = self.pages[index].as_ref().unwrap();
        let (shelf, xy) = page.shelf.place(width, height, options.page_size).unwrap();
        uploads.append(
            index as u32,
            xy,
            glyph,
            options.upload_bytes,
            options.max_entries,
        )?;
        let atlas = AtlasGlyph {
            placement: glyph.placement,
            page: index as u32,
            rect: [
                xy[0] as f32,
                xy[1] as f32,
                glyph.placement.width as f32,
                glyph.placement.height as f32,
            ],
            content: glyph.content,
        };
        let key = if glyph.content == Content::Mask {
            key.mask()
        } else {
            key
        };
        let hash = self.hash.hash_one(key);
        self.entries.insert_unique(
            hash,
            Entry {
                key: key.to_owned(),
                hash,
                glyph: atlas,
            },
            |entry| entry.hash,
        );
        let page = self.pages[index].as_mut().unwrap();
        page.shelf = shelf;
        page.entries += 1;
        page.dirty = true;
        self.touch(index as u32);
        Ok(atlas)
    }
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
        let budget = budget - self.uploads.device_bytes();
        self.storage
            .insert(
                key,
                glyph,
                &mut self.uploads,
                self.options,
                device,
                pipeline,
                budget,
            )
            .map(Some)
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
            atlas_entries: self.storage.entries.len(),
            atlas_bytes: self.storage.bytes(),
            staging_bytes: self.uploads.device_bytes(),
            upload_bytes: self.uploads.bytes.capacity(),
            upload_regions: self.uploads.regions.len(),
            upload_region_capacity: self.uploads.regions.capacity(),
            entry_capacity: self.storage.entries.capacity(),
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
