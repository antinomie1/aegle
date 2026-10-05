//! Shelf-packed atlas pages with whole-page LRU eviction.
use std::{collections::hash_map::RandomState, hash::BuildHasher};

use aegle_glyph::{Content, Glyph, GlyphKey, OwnedGlyphKey, Placement};
use aegle_gpu::Shelf;
use ash::vk;
use hashbrown::HashTable;

use crate::{
    Error, Result, TextOptions, device::Device, memory::Image, text_pipeline::TextPipeline,
    upload::Uploads,
};

pub(crate) use aegle_gpu::ResourceKey;

pub(crate) enum EntryKey {
    Glyph(OwnedGlyphKey),
    Resource(ResourceKey),
}

#[derive(Clone, Copy)]
pub(crate) struct AtlasGlyph {
    pub placement: Placement,
    pub page: u32,
    pub rect: [f32; 4],
    pub content: Content,
}

struct Entry {
    key: EntryKey,
    hash: u64,
    glyph: AtlasGlyph,
}

pub(crate) struct Page {
    pub image: Image,
    content: Content,
    extent: [u32; 2],
    shelf: Shelf,
    used: u64,
    entries: u32,
    pub pinned: bool,
    pub initialized: bool,
    pub reset: bool,
    pub dirty: bool,
}

impl Page {
    fn fits(&self, content: Content, extent: [u32; 2]) -> bool {
        self.content == content
            && self
                .shelf
                .place(extent[0], extent[1], self.extent)
                .is_some()
    }
}

pub(crate) struct Storage {
    pub pages: Vec<Option<Page>>,
    entries: HashTable<Entry>,
    hash: RandomState,
    clock: u64,
}
impl Storage {
    pub fn new() -> Self {
        Self {
            pages: Vec::new(),
            entries: HashTable::new(),
            hash: RandomState::new(),
            clock: 0,
        }
    }

    pub fn bytes(&self) -> u64 {
        self.pages
            .iter()
            .flatten()
            .map(|p| p.image.allocation)
            .sum()
    }

    pub fn entries(&self) -> usize {
        self.entries.len()
    }

    pub fn entry_capacity(&self) -> usize {
        self.entries.capacity()
    }

    pub fn touch(&mut self, index: u32) {
        self.clock = self.clock.wrapping_add(1);
        let page = self.pages[index as usize].as_mut().unwrap();
        page.used = self.clock;
        page.pinned = true;
    }

    pub fn hash(&self, key: impl std::hash::Hash) -> u64 {
        self.hash.hash_one(key)
    }

    pub fn lookup(&self, key: GlyphKey<'_>) -> Option<AtlasGlyph> {
        let glyph = |key: GlyphKey<'_>, mask: bool| {
            self.entries
                .find(self.hash(key), |entry| {
                    matches!(&entry.key, EntryKey::Glyph(owned)
                        if owned.as_key() == key
                            && (!mask || entry.glyph.content == Content::Mask))
                })
                .map(|entry| entry.glyph)
        };
        glyph(key.mask(), true).or_else(|| glyph(key, false))
    }

    pub fn lookup_resource(&self, key: ResourceKey) -> Option<AtlasGlyph> {
        self.entries
            .find(
                self.hash(key),
                |entry| matches!(entry.key, EntryKey::Resource(owned) if owned == key),
            )
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

    pub fn reset(&mut self, index: usize) {
        self.entries
            .retain(|entry| entry.glyph.page as usize != index);
        let page = self.pages[index].as_mut().unwrap();
        page.shelf = Shelf::default();
        page.entries = 0;
        page.reset = true;
        page.dirty = false;
    }

    /// Finds or allocates a page for `extent`. Entries larger than a standard
    /// page get a page of exactly their extent from the same page and byte limits.
    fn page(
        &mut self,
        content: Content,
        extent: [u32; 2],
        options: TextOptions,
        device: &Device,
        pipeline: &TextPipeline,
        budget: u64,
    ) -> Result<usize> {
        if let Some(index) = self
            .pages
            .iter()
            .position(|page| page.as_ref().is_some_and(|p| p.fits(content, extent)))
        {
            return Ok(index);
        }
        let side = options.page_size;
        let size = if extent[0] <= side && extent[1] <= side {
            [side, side]
        } else {
            extent
        };
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
                    size[0],
                    size[1],
                    format,
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    budget - self.bytes(),
                ) {
                    Ok(image) => {
                        pipeline.update(index as u32, image.view);
                        let page = Some(Page {
                            image,
                            content,
                            extent: size,
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
            if self.pages[index].as_ref().unwrap().fits(content, extent) {
                return Ok(index);
            }
            self.pages[index] = None;
        }
    }

    /// Places `glyph` with a transparent one-pixel border and queues its upload.
    #[allow(clippy::too_many_arguments)]
    pub fn insert(
        &mut self,
        key: EntryKey,
        hash: u64,
        glyph: Glyph<'_>,
        uploads: &mut Uploads,
        options: TextOptions,
        device: &Device,
        pipeline: &TextPipeline,
        budget: u64,
    ) -> Result<AtlasGlyph> {
        let border = |value: u32| value.checked_add(2).ok_or(Error::GlyphTooLarge);
        let extent = [
            border(glyph.placement.width)?,
            border(glyph.placement.height)?,
        ];
        if matches!(key, EntryKey::Glyph(_))
            && (extent[0] > options.page_size || extent[1] > options.page_size)
        {
            return Err(Error::GlyphTooLarge);
        }
        while self.entries.len() >= options.max_entries as usize {
            let index = self.oldest(true).ok_or(Error::AtlasFull)?;
            self.reset(index);
        }
        self.entries
            .try_reserve(1, |entry| entry.hash)
            .map_err(|_| Error::Allocation)?;
        let index = self.page(glyph.content, extent, options, device, pipeline, budget)?;
        let page = self.pages[index].as_ref().unwrap();
        let (shelf, xy) = page.shelf.place(extent[0], extent[1], page.extent).unwrap();
        // The glyph sits inside its one-pixel transparent border.
        let xy = [xy[0] + 1, xy[1] + 1];
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
        self.entries.insert_unique(
            hash,
            Entry {
                key,
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
