//! Images and CPU-rasterized path coverage drawn from the shared atlas.
use aegle_glyph::{Content, Glyph, Placement};
use aegle_gpu::{State, Textured, image_placement, path_raster, rasterize_path};
use aegle_scene::{Affine, Color, Image, Path, Rect, Stroke};
use aegle_types::color_math::linear_rgba;

use crate::{Renderer, Result, atlas::AtlasGlyph};

impl Renderer {
    pub(crate) fn image(&mut self, image: &Image, rect: Rect, state: State) -> Result {
        let Some(placed) = image_placement(image, rect, state)? else {
            return Ok(());
        };
        let entry = match self.atlas.resource(placed.key) {
            Some(entry) => entry,
            None => self.place(
                placed.key,
                Glyph {
                    placement: Placement {
                        left: 0,
                        top: 0,
                        width: image.width(),
                        height: image.height(),
                    },
                    content: Content::Color,
                    data: image.pixels(),
                },
            )?,
        };
        self.emit(entry, placed.area, placed.inverse, [1.0; 4], 3, state)
    }

    pub(crate) fn path(
        &mut self,
        path: &Path,
        color: Color,
        stroke: Option<Stroke>,
        state: State,
    ) -> Result {
        if color.to_rgba()[3] == 0 {
            return Ok(());
        }
        let Some(raster) = path_raster(path, stroke, state)? else {
            return Ok(());
        };
        let entry = match self.atlas.resource(raster.key) {
            Some(entry) => entry,
            None => {
                let (data, placement) =
                    rasterize_path(path, &raster, state.transform, &mut self.atlas.scratch);
                if placement.width == 0 || placement.height == 0 {
                    return Ok(());
                }
                self.place(
                    raster.key,
                    Glyph {
                        placement,
                        content: Content::Mask,
                        data: &data,
                    },
                )?
            }
        };
        let left = raster.origin[0] + entry.placement.left as f32;
        let top = raster.origin[1] + entry.placement.top as f32;
        let area = [
            left,
            top,
            left + entry.placement.width as f32,
            top + entry.placement.height as f32,
        ];
        let inverse = Affine::translation(-left, -top)?;
        self.emit(entry, area, inverse, linear_rgba(color.to_rgba()), 1, state)
    }

    /// `inverse` maps device pixels into the entry's texel coordinates. Masks use
    /// kind 1; color entries are images with clamped edges (kind 3).
    fn emit(
        &mut self,
        entry: AtlasGlyph,
        area: [f32; 4],
        inverse: Affine,
        color: [f32; 4],
        kind: u32,
        state: State,
    ) -> Result {
        self.rec.record(
            Textured {
                area,
                inverse,
                rect: entry.rect,
                contrast: 0.0,
                viewport: self.viewport,
                color,
                kind,
                page: entry.slot.page(),
            }
            .primitive(state.clip),
            state.bounds,
        )?;
        self.flush_full()
    }
}
