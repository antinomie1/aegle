//! Glyph runs and the shared atlas upload with page recycling.
use aegle_glyph::{Glyph, Placement, RasterOptions, RasterTransform, mask_contrast};
use aegle_gpu::{State, Textured, bounds};
use aegle_scene::{GlyphRun, Rect, RoundedRect};
use aegle_types::color_math::linear_rgba;

use crate::{
    Error, Renderer, Result,
    atlas::{AtlasGlyph, Fetch, ResourceKey, Slot},
};

impl Renderer {
    pub(crate) fn glyphs(&mut self, run: &GlyphRun, state: State) -> Result {
        let [r, g, b, alpha] = run.color().to_rgba();
        if alpha == 0 || state.bounds[0] >= state.bounds[2] || state.bounds[1] >= state.bounds[3] {
            return Ok(());
        }
        let viewport = self.viewport;
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
                Ok(aegle_gpu::visible(area, state.bounds))
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
            let mask = image.slot == Slot::Mask;
            self.rec.record(
                Textured {
                    area,
                    inverse: transform.inverse()?,
                    rect: image.rect,
                    contrast,
                    viewport,
                    color: if mask {
                        linear_rgba(run.color().to_rgba())
                    } else {
                        [f32::from(alpha) / 255.0; 4]
                    },
                    kind: if mask { 1 } else { 2 },
                    page: image.slot.page(),
                }
                .primitive(state.clip),
                state.bounds,
            )?;
            self.flush_full()?;
        }
        Ok(())
    }

    /// Uploads an image or path mask. A full page is cleared after submitting
    /// the draws that still reference it, then the upload is retried once.
    pub(crate) fn place(&mut self, key: ResourceKey, glyph: Glyph<'_>) -> Result<AtlasGlyph> {
        for _ in 0..2 {
            match self.atlas.insert_resource(&self.gpu, key, glyph)? {
                Ok(entry) => return Ok(entry),
                Err(crate::atlas::Full(mask)) => {
                    self.flush(None)?;
                    self.atlas.reset(mask);
                }
            }
        }
        Err(Error::TooLarge)
    }
}
