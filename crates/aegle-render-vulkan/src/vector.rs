//! Images and CPU-rasterized path coverage drawn from the shared atlas.
use aegle_glyph::{Content, Glyph, Placement};
use aegle_gpu::{Recording, State, Textured, image_placement, path_raster, rasterize_path};
use aegle_scene::{Affine, Color, Image, Path, Rect, Stroke};
use aegle_types::color_math::linear_rgba;

use crate::{
    Result,
    atlas::AtlasGlyph,
    device::Device,
    text::{Limits, Text},
};

impl Text {
    pub(crate) fn image(
        &mut self,
        device: &Device,
        recording: &mut Recording,
        image: &Image,
        rect: Rect,
        state: State,
        limits: Limits,
    ) -> Result {
        let Some(placed) = image_placement(image, rect, state)? else {
            return Ok(());
        };
        let entry = match self.atlas.resource(placed.key) {
            Some(entry) => entry,
            None => self.atlas.insert_resource(
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
                device,
                &self.pipeline,
                limits.device,
            )?,
        };
        record(
            recording,
            entry,
            placed.area,
            placed.inverse,
            [1.0; 4],
            state,
            limits,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn path(
        &mut self,
        device: &Device,
        recording: &mut Recording,
        path: &Path,
        color: Color,
        stroke: Option<Stroke>,
        state: State,
        limits: Limits,
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
                    rasterize_path(path, &raster, state.transform, &mut self.scratch);
                if placement.width == 0 || placement.height == 0 {
                    return Ok(());
                }
                self.atlas.insert_resource(
                    raster.key,
                    Glyph {
                        placement,
                        content: Content::Mask,
                        data: &data,
                    },
                    device,
                    &self.pipeline,
                    limits.device,
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
        record(
            recording,
            entry,
            area,
            inverse,
            linear_rgba(color.to_rgba()),
            state,
            limits,
        )
    }
}

/// `inverse` maps device pixels into the entry's texel coordinates. Path masks
/// use kind 1; color entries are images with clamped edges (kind 3).
fn record(
    recording: &mut Recording,
    entry: AtlasGlyph,
    area: [f32; 4],
    inverse: Affine,
    color: [f32; 4],
    state: State,
    limits: Limits,
) -> Result {
    recording.record(
        Textured {
            area,
            inverse,
            rect: entry.rect,
            contrast: 0.0,
            viewport: limits.viewport,
            color,
            kind: if entry.content == Content::Mask { 1 } else { 3 },
            page: entry.page,
        }
        .primitive(state.clip),
        state.bounds,
    )?;
    Ok(())
}
