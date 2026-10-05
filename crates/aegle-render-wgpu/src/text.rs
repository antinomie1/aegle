//! Glyph atlas: one mask page and one color page, shelf packed, uploaded on demand.
//! A full page is cleared as a whole after the frame's pending draws are submitted.
use std::{collections::hash_map::RandomState, hash::BuildHasher};

use aegle_glyph::{
    Content, FontData, Glyph, GlyphCache, GlyphKey, OwnedGlyphKey, Placement, RasterOptions,
    RasterTransform, mask_contrast,
};
use aegle_scene::{GlyphRun, Rect, RoundedRect};
use aegle_types::color_math::{SrgbTransfer, linear_rgba};
use hashbrown::HashTable;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, Extent3d, Origin3d,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};

use crate::{
    Error, Renderer, Result,
    gpu::Gpu,
    records::{Kind, Primitive, State, bounds},
};

/// Resident glyphs per page, bounding table memory independent of page area.
const MAX_ENTRIES: usize = 4096;

#[derive(Clone, Copy)]
pub(crate) struct AtlasGlyph {
    placement: Placement,
    /// Origin and size inside the page, excluding the transparent border.
    rect: [f32; 4],
    mask: bool,
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
    /// Advances past a `width × height` block, wrapping to a new row.
    fn place(&mut self, width: u32, height: u32, side: u32) -> Option<[u32; 2]> {
        let mut next = *self;
        if next.x + width > side {
            next.x = 0;
            next.y += next.height;
            next.height = 0;
        }
        if next.y + height > side {
            return None;
        }
        let at = [next.x, next.y];
        next.x += width;
        next.height = next.height.max(height);
        *self = next;
        Some(at)
    }
}

struct PageGpu {
    texture: Texture,
    group: BindGroup,
}

#[derive(Default)]
struct Page {
    gpu: Option<PageGpu>,
    shelf: Shelf,
    entries: HashTable<Entry>,
}

pub(crate) enum Fetch {
    Ready(AtlasGlyph),
    Hidden,
    /// The mask (true) or color page needs clearing before this glyph fits.
    Full(bool),
}

pub(crate) struct Atlas {
    side: u32,
    cache: GlyphCache,
    mask: Page,
    color: Page,
    hash: RandomState,
    scratch: Vec<u8>,
}

impl Atlas {
    pub fn new(side: u32) -> Self {
        Self {
            side,
            cache: GlyphCache::with_limits(Default::default()),
            mask: Page::default(),
            color: Page::default(),
            hash: RandomState::new(),
            scratch: Vec::new(),
        }
    }

    pub fn group(&self, mask: bool) -> &BindGroup {
        let page = if mask { &self.mask } else { &self.color };
        &page.gpu.as_ref().unwrap().group
    }

    /// Forgets a page's glyphs; its texture stays allocated and is overwritten.
    pub fn reset(&mut self, mask: bool) {
        let page = if mask {
            &mut self.mask
        } else {
            &mut self.color
        };
        page.entries.clear();
        page.shelf = Shelf::default();
    }

    fn lookup(&self, key: GlyphKey<'_>) -> Option<AtlasGlyph> {
        let find = |page: &Page, key: GlyphKey<'_>| {
            page.entries
                .find(self.hash.hash_one(key), |entry| entry.key.as_key() == key)
                .map(|entry| entry.glyph)
        };
        find(&self.mask, key.mask()).or_else(|| find(&self.color, key))
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
        if let Some(glyph) = self.lookup(key) {
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
        let mask = glyph.content == Content::Mask;
        let key = if mask { key.mask() } else { key };
        let extent = [glyph.placement.width + 2, glyph.placement.height + 2];
        if extent[0] > self.side || extent[1] > self.side {
            return Err(Error::GlyphTooLarge);
        }
        let page = if mask {
            &mut self.mask
        } else {
            &mut self.color
        };
        let Some(at) = page
            .shelf_place(extent, self.side)
            .filter(|_| page.entries.len() < MAX_ENTRIES)
        else {
            return Ok(Fetch::Full(mask));
        };
        let upload = page.texture(gpu, self.side, mask);
        write(gpu, upload, at, glyph, &mut self.scratch);
        let glyph = AtlasGlyph {
            placement: glyph.placement,
            rect: [
                (at[0] + 1) as f32,
                (at[1] + 1) as f32,
                glyph.placement.width as f32,
                glyph.placement.height as f32,
            ],
            mask,
        };
        let hash = self.hash.hash_one(key);
        page.entries.insert_unique(
            hash,
            Entry {
                key: key.to_owned(),
                hash,
                glyph,
            },
            |entry| entry.hash,
        );
        Ok(Fetch::Ready(glyph))
    }
}

impl Page {
    fn shelf_place(&mut self, extent: [u32; 2], side: u32) -> Option<[u32; 2]> {
        self.shelf.place(extent[0], extent[1], side)
    }

    fn texture(&mut self, gpu: &Gpu, side: u32, mask: bool) -> &Texture {
        &self
            .gpu
            .get_or_insert_with(|| {
                let texture = gpu.device.create_texture(&TextureDescriptor {
                    label: Some("aegle glyph page"),
                    size: Extent3d {
                        width: side,
                        height: side,
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
                    label: Some("aegle glyph page"),
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
                PageGpu { texture, group }
            })
            .texture
    }
}

/// Uploads `glyph` surrounded by a transparent one-pixel border at `at`.
fn write(gpu: &Gpu, texture: &Texture, at: [u32; 2], glyph: Glyph<'_>, scratch: &mut Vec<u8>) {
    let bytes = if glyph.content == Content::Mask { 1 } else { 4 };
    let [width, height] = [glyph.placement.width, glyph.placement.height].map(|v| v as usize);
    let stride = (width + 2) * bytes;
    scratch.clear();
    scratch.resize(stride * (height + 2), 0);
    let transfer = SrgbTransfer::get();
    for (row, source) in glyph.data.chunks_exact(width * bytes).enumerate() {
        let dest = &mut scratch[(row + 1) * stride + bytes..][..width * bytes];
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
        scratch,
        TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride as u32),
            rows_per_image: None,
        },
        Extent3d {
            width: width as u32 + 2,
            height: height as u32 + 2,
            depth_or_array_layers: 1,
        },
    );
}

impl Renderer {
    pub(crate) fn glyphs(&mut self, run: &GlyphRun, state: State) -> Result {
        let [r, g, b, alpha] = run.color().to_rgba();
        if alpha == 0 || state.bounds[0] >= state.bounds[2] || state.bounds[1] >= state.bounds[3] {
            return Ok(());
        }
        let viewport = self.size.map(|v| v as f32);
        let raster = RasterTransform::new(state.transform, run.size())?;
        let contrast = mask_contrast([r, g, b, alpha]);
        for glyph in run.glyphs() {
            let origin = raster.origin(glyph.position)?;
            let options = RasterOptions {
                size: raster.size(),
                offset: origin.offset(),
                normalized_coords: run.normalized_coords(),
                hint: raster.hint(),
                foreground: [r, g, b, 255],
            };
            let mut geometry = None;
            let mut visible = |placement: Placement| {
                let transform = origin.image_transform(placement)?;
                let shape = RoundedRect::new(
                    Rect::new(0.0, 0.0, placement.width as f32, placement.height as f32),
                    0.0,
                )?;
                // Bitmap filtering already contributes its half-pixel support;
                // the analytic geometry AA fringe must not pin invisible glyphs.
                let area = bounds(shape, transform, if raster.hint() { 0.0 } else { 0.5 }, 0.0)?;
                geometry = Some((transform, area));
                Ok(area[0].floor() < state.bounds[2]
                    && area[2].ceil() > state.bounds[0]
                    && area[1].floor() < state.bounds[3]
                    && area[3].ceil() > state.bounds[1])
            };
            let mut fetched =
                self.atlas
                    .fetch(&self.gpu, run.font(), glyph.id, options, &mut visible)?;
            if let Fetch::Full(mask) = fetched {
                // Earlier draws still reference the page: submit them, then reuse it.
                self.flush(None)?;
                self.atlas.reset(mask);
                fetched =
                    self.atlas
                        .fetch(&self.gpu, run.font(), glyph.id, options, &mut visible)?;
            }
            let Fetch::Ready(image) = fetched else {
                continue;
            };
            let (transform, area) = geometry.unwrap();
            let [a, b, c, d, e, f] = transform.inverse()?.coefficients();
            self.rec.record(
                Primitive {
                    bounds: area,
                    row0: [a, c, e, 0.0],
                    row1: [b, d, f, 0.0],
                    rect: image.rect,
                    params: [contrast, 0.0, viewport[0], viewport[1]],
                    color: if image.mask {
                        linear_rgba(run.color().to_rgba())
                    } else {
                        [f32::from(alpha) / 255.0; 4]
                    },
                    header: [state.clip, if image.mask { 1 } else { 2 }, 0, 0],
                },
                state.bounds,
                if image.mask { Kind::Mask } else { Kind::Color },
            );
            self.flush_full()?;
        }
        Ok(())
    }
}
