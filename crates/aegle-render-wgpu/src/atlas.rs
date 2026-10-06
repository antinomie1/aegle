//! Glyph, image and path-mask storage: one shelf-packed mask page and one color
//! page, plus exact-size textures for entries too large for a page. A full page
//! is cleared as a whole after the frame's pending draws are submitted.
use std::{collections::hash_map::RandomState, hash::BuildHasher};

use aegle_glyph::{
    Content, FontData, Glyph, GlyphCache, GlyphKey, OwnedGlyphKey, Placement, RasterOptions,
};
use aegle_types::color_math::SrgbTransfer;
use hashbrown::HashTable;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, Extent3d, Origin3d,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};

use crate::{Error, Result, gpu::Gpu};
pub(crate) use aegle_gpu::{ResourceKey, Shelf};

/// Resident entries per page, bounding table memory independent of page area.
const MAX_ENTRIES: usize = 4096;
/// Conversion scratch kept between uploads; larger images free it afterwards.
const SCRATCH_RETAIN: usize = 1 << 20;

enum EntryKey {
    Glyph(OwnedGlyphKey),
    Resource(ResourceKey),
}

/// Which texture holds an entry; also selects its bind group.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Slot {
    Mask,
    Color,
    /// Index into the exact-size textures.
    Own(u32),
    /// Index into the application textures recorded since the last submission.
    External(u32),
}

/// Page-field bit marking an application texture.
const EXTERNAL: u32 = 1 << 31;

impl Slot {
    /// Selector stored in a primitive's page field, read back when batching.
    pub fn page(self) -> u32 {
        match self {
            Self::Mask => 0,
            Self::Color => 1,
            Self::Own(index) => 2 + index,
            Self::External(index) => EXTERNAL | index,
        }
    }

    pub fn from_page(page: u32) -> Self {
        match page {
            0 => Self::Mask,
            1 => Self::Color,
            page if page & EXTERNAL != 0 => Self::External(page & !EXTERNAL),
            _ => Self::Own(page - 2),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AtlasGlyph {
    pub placement: Placement,
    /// Origin and size inside the texture, excluding any transparent border.
    pub rect: [f32; 4],
    pub slot: Slot,
}

struct Entry {
    key: EntryKey,
    hash: u64,
    glyph: AtlasGlyph,
}

struct PageGpu {
    texture: Texture,
    group: BindGroup,
}

impl PageGpu {
    fn new(gpu: &Gpu, size: [u32; 2], mask: bool) -> Self {
        let texture = gpu.device.create_texture(&TextureDescriptor {
            label: Some("aegle atlas"),
            size: Extent3d {
                width: size[0],
                height: size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            // sRGB decode on sampling yields premultiplied linear color.
            format: if mask {
                TextureFormat::R8Unorm
            } else {
                TextureFormat::Rgba8UnormSrgb
            },
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let group = gpu.device.create_bind_group(&BindGroupDescriptor {
            label: Some("aegle atlas"),
            layout: &gpu.pages,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&gpu.sampler),
                },
            ],
        });
        Self { texture, group }
    }
}

#[derive(Default)]
struct Page {
    gpu: Option<PageGpu>,
    shelf: Shelf,
    entries: HashTable<Entry>,
}

struct Own {
    gpu: PageGpu,
    /// Frame counter value of the last lookup or insert.
    used: u64,
}

/// A placement that did not fit: the mask (true) or color page needs clearing.
pub(crate) struct Full(pub bool);
pub(crate) type Placed = std::result::Result<AtlasGlyph, Full>;

pub(crate) enum Fetch {
    Ready(AtlasGlyph),
    Hidden,
    Full(bool),
}

struct Store {
    side: u32,
    mask: Page,
    color: Page,
    own: Vec<Option<Own>>,
    own_entries: HashTable<Entry>,
    hash: RandomState,
    scratch: Vec<u8>,
    frame: u64,
}

pub(crate) struct Atlas {
    cache: GlyphCache,
    store: Store,
    /// Reused path-mask rasterizer storage.
    pub scratch: aegle_gpu::Scratch,
}

impl Atlas {
    pub fn new(side: u32) -> Self {
        Self {
            cache: GlyphCache::with_limits(Default::default()),
            store: Store {
                side,
                mask: Page::default(),
                color: Page::default(),
                own: Vec::new(),
                own_entries: HashTable::new(),
                hash: RandomState::new(),
                scratch: Vec::new(),
                frame: 0,
            },
            scratch: aegle_gpu::Scratch::new(),
        }
    }

    /// Exact-size textures survive one frame without use, so an animation that
    /// redraws them keeps its uploads while a vanished image is released.
    pub fn begin_frame(&mut self) {
        let store = &mut self.store;
        store.frame += 1;
        let frame = store.frame;
        let own = &mut store.own;
        store.own_entries.retain(|entry| {
            let Slot::Own(index) = entry.glyph.slot else {
                return true;
            };
            let keep = own[index as usize].as_ref().unwrap().used + 1 >= frame;
            if !keep {
                own[index as usize] = None;
            }
            keep
        });
    }

    pub fn group(&self, slot: Slot) -> &BindGroup {
        let store = &self.store;
        match slot {
            Slot::Mask => &store.mask.gpu.as_ref().unwrap().group,
            Slot::Color => &store.color.gpu.as_ref().unwrap().group,
            Slot::Own(index) => &store.own[index as usize].as_ref().unwrap().gpu.group,
            Slot::External(_) => unreachable!("application textures bind from the renderer"),
        }
    }

    /// Entries currently resident in pages or exact-size textures.
    pub fn len(&self) -> usize {
        let store = &self.store;
        store.mask.entries.len() + store.color.entries.len() + store.own_entries.len()
    }

    /// Forgets a page's entries; its texture stays allocated and is overwritten.
    pub fn reset(&mut self, mask: bool) {
        let store = &mut self.store;
        let page = if mask {
            &mut store.mask
        } else {
            &mut store.color
        };
        page.entries.clear();
        page.shelf = Shelf::default();
    }

    /// Finds or uploads one glyph. `visible` reports whether its placement can
    /// reach the clip, so off-screen glyphs never occupy the atlas.
    pub fn fetch(
        &mut self,
        gpu: &Gpu,
        font: &FontData,
        id: u16,
        options: RasterOptions<'_>,
        mut visible: impl FnMut(Placement) -> Result<bool>,
    ) -> Result<Fetch> {
        let key = GlyphKey::new(font, id, options)?;
        let store = &mut self.store;
        let hash_of = |store: &Store, key: GlyphKey<'_>| store.hash.hash_one(key);
        let find = |store: &Store, page: &Page, key: GlyphKey<'_>| {
            page.entries
                .find(
                    hash_of(store, key),
                    |entry| matches!(&entry.key, EntryKey::Glyph(owned) if owned.as_key() == key),
                )
                .map(|entry| entry.glyph)
        };
        if let Some(glyph) =
            find(store, &store.mask, key.mask()).or_else(|| find(store, &store.color, key))
        {
            return Ok(if visible(glyph.placement)? {
                Fetch::Ready(glyph)
            } else {
                Fetch::Hidden
            });
        }
        let glyph = self.cache.rasterize(font, id, options)?;
        if glyph.placement.width == 0 || glyph.placement.height == 0 || !visible(glyph.placement)? {
            return Ok(Fetch::Hidden);
        }
        let key = if glyph.content == Content::Mask {
            key.mask()
        } else {
            key
        };
        let hash = hash_of(store, key);
        let limit = gpu.device.limits().max_texture_dimension_2d;
        Ok(
            match store.place(gpu, EntryKey::Glyph(key.to_owned()), hash, glyph, limit)? {
                Ok(glyph) => Fetch::Ready(glyph),
                Err(Full(mask)) => Fetch::Full(mask),
            },
        )
    }

    /// Returns a resident image or path mask and marks it used this frame.
    pub fn resource(&mut self, key: ResourceKey) -> Option<AtlasGlyph> {
        let store = &mut self.store;
        let hash = store.hash.hash_one(key);
        let find = |page: &Page| {
            page.entries
                .find(
                    hash,
                    |entry| matches!(&entry.key, EntryKey::Resource(have) if *have == key),
                )
                .map(|entry| entry.glyph)
        };
        let own = store
            .own_entries
            .find(
                hash,
                |entry| matches!(&entry.key, EntryKey::Resource(have) if *have == key),
            )
            .map(|entry| entry.glyph);
        if let Some(AtlasGlyph {
            slot: Slot::Own(index),
            ..
        }) = own
        {
            store.own[index as usize].as_mut().unwrap().used = store.frame;
        }
        own.or_else(|| find(&store.mask))
            .or_else(|| find(&store.color))
    }

    /// Uploads an image (color) or path mask missing from [`Self::resource`].
    pub fn insert_resource(
        &mut self,
        gpu: &Gpu,
        key: ResourceKey,
        glyph: Glyph<'_>,
    ) -> Result<Placed> {
        let store = &mut self.store;
        let hash = store.hash.hash_one(key);
        let limit = gpu.device.limits().max_texture_dimension_2d;
        store.place(gpu, EntryKey::Resource(key), hash, glyph, limit)
    }
}

impl Store {
    /// Places `glyph` in a page, or in an exact-size texture when it exceeds a
    /// page and is a resource. Nothing changes when the page is full.
    fn place(
        &mut self,
        gpu: &Gpu,
        key: EntryKey,
        hash: u64,
        glyph: Glyph<'_>,
        limit: u32,
    ) -> Result<Placed> {
        let Placement { width, height, .. } = glyph.placement;
        let mask = glyph.content == Content::Mask;
        let (extent, side) = ([width + 2, height + 2], self.side);
        let slot_glyph;
        if extent[0] <= side && extent[1] <= side {
            let page = if mask {
                &mut self.mask
            } else {
                &mut self.color
            };
            if page.entries.len() >= MAX_ENTRIES {
                return Ok(Err(Full(mask)));
            }
            let Some((shelf, at)) = page.shelf.place(extent[0], extent[1], [side, side]) else {
                return Ok(Err(Full(mask)));
            };
            page.shelf = shelf;
            let page_gpu = page
                .gpu
                .get_or_insert_with(|| PageGpu::new(gpu, [side, side], mask));
            write(gpu, &page_gpu.texture, at, glyph, &mut self.scratch, true);
            slot_glyph = AtlasGlyph {
                placement: glyph.placement,
                rect: [
                    (at[0] + 1) as f32,
                    (at[1] + 1) as f32,
                    width as f32,
                    height as f32,
                ],
                slot: if mask { Slot::Mask } else { Slot::Color },
            };
            page.entries.insert_unique(
                hash,
                Entry {
                    key,
                    hash,
                    glyph: slot_glyph,
                },
                |entry| entry.hash,
            );
            return Ok(Ok(slot_glyph));
        }
        if matches!(key, EntryKey::Glyph(_)) || width > limit || height > limit {
            return Err(Error::TooLarge);
        }
        let own = Own {
            gpu: PageGpu::new(gpu, [width, height], mask),
            used: self.frame,
        };
        write(
            gpu,
            &own.gpu.texture,
            [0, 0],
            glyph,
            &mut self.scratch,
            false,
        );
        let index = match self.own.iter().position(Option::is_none) {
            Some(index) => {
                self.own[index] = Some(own);
                index
            }
            None => {
                self.own.push(Some(own));
                self.own.len() - 1
            }
        };
        slot_glyph = AtlasGlyph {
            placement: glyph.placement,
            rect: [0.0, 0.0, width as f32, height as f32],
            slot: Slot::Own(index as u32),
        };
        self.own_entries.insert_unique(
            hash,
            Entry {
                key,
                hash,
                glyph: slot_glyph,
            },
            |entry| entry.hash,
        );
        Ok(Ok(slot_glyph))
    }
}

/// Uploads `glyph` at `at`; with `border`, surrounded by a transparent pixel.
fn write(
    gpu: &Gpu,
    texture: &Texture,
    at: [u32; 2],
    glyph: Glyph<'_>,
    scratch: &mut Vec<u8>,
    border: bool,
) {
    let bytes = if glyph.content == Content::Mask { 1 } else { 4 };
    let [width, height] = [glyph.placement.width, glyph.placement.height].map(|v| v as usize);
    let pad = usize::from(border) * 2;
    let stride = (width + pad) * bytes;
    let inset = usize::from(border);
    // A grayscale entry without a border uploads straight from the source.
    let direct = bytes == 1 && !border;
    if !direct {
        scratch.clear();
        scratch.resize(stride * (height + pad), 0);
        let transfer = SrgbTransfer::get();
        for (row, source) in glyph.data.chunks_exact(width * bytes).enumerate() {
            let dest = &mut scratch[(row + inset) * stride + inset * bytes..][..width * bytes];
            if bytes == 1 {
                dest.copy_from_slice(source);
                continue;
            }
            // Premultiply in linear light, then sRGB-encode so the texture decodes
            // to the premultiplied value before filtering. Alpha stays linear.
            for (source, dest) in source.chunks_exact(4).zip(dest.chunks_exact_mut(4)) {
                let alpha = source[3] as f32 / 255.0;
                for c in 0..3 {
                    dest[c] = (transfer.encode(transfer.decode(source[c] as f32 / 255.0) * alpha)
                        * 255.0)
                        .round() as u8;
                }
                dest[3] = source[3];
            }
        }
    }
    gpu.queue.write_texture(
        TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: Origin3d {
                x: at[0],
                y: at[1],
                z: 0,
            },
            aspect: TextureAspect::All,
        },
        if direct { glyph.data } else { scratch },
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride as u32),
            rows_per_image: None,
        },
        Extent3d {
            width: (width + pad) as u32,
            height: (height + pad) as u32,
            depth_or_array_layers: 1,
        },
    );
    if scratch.capacity() > SCRATCH_RETAIN {
        *scratch = Vec::new();
    }
}
