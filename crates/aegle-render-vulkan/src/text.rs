use aegle_glyph::{Content, RasterOptions, RasterTransform};
use aegle_scene::{GlyphRun, Rect, RoundedRect};
use aegle_types::color_math::linear_rgba;

use crate::{
    Error, Result, TextOptions,
    atlas::Atlas,
    device::Device,
    geometry::{Primitive, Recording, State, bounds},
    pipeline::Pipeline,
    text_pipeline::TextPipeline,
};

pub(crate) struct Text {
    // Descriptor views are released before the descriptor pool and sampler.
    pub atlas: Atlas,
    pub pipeline: TextPipeline,
}

impl Text {
    pub fn new(device: &Device, pipeline: &Pipeline, options: TextOptions) -> Result<Self> {
        let limits = device.properties.limits;
        if options.page_size > limits.max_image_dimension2_d
            || options.max_pages > limits.max_memory_allocation_count.saturating_sub(5)
        {
            return Err(Error::InvalidTextOptions);
        }
        Ok(Self {
            atlas: Atlas::new(options)?,
            pipeline: TextPipeline::new(device, pipeline, options.max_pages)?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        device: &Device,
        recording: &mut Recording,
        run: &GlyphRun,
        state: State,
        width: u32,
        height: u32,
        device_budget: u64,
        recording_budget: usize,
    ) -> Result {
        let [r, g, b, alpha] = run.color().to_rgba();
        if alpha == 0 || state.bounds[0] >= state.bounds[2] || state.bounds[1] >= state.bounds[3] {
            return Ok(());
        }
        let raster = RasterTransform::new(state.transform, run.size())?;
        let contrast = aegle_glyph::mask_contrast([r, g, b, alpha]);
        for glyph in run.glyphs() {
            let origin = raster.origin(glyph.position)?;
            let mut geometry = None;
            let Some(image) = self.atlas.get(
                run.font(),
                glyph.id,
                RasterOptions {
                    size: raster.size(),
                    offset: origin.offset(),
                    normalized_coords: run.normalized_coords(),
                    hint: raster.hint(),
                    foreground: [r, g, b, 255],
                },
                device,
                &self.pipeline,
                device_budget,
                |placement| {
                    let transform = origin.image_transform(placement)?;
                    let shape = RoundedRect::new(
                        Rect::new(0.0, 0.0, placement.width as f32, placement.height as f32),
                        0.0,
                    )?;
                    // Bitmap filtering already contributes its half-pixel support;
                    // the analytic geometry AA fringe must not pin invisible glyphs.
                    let bounds =
                        bounds(shape, transform, if raster.hint() { 0.0 } else { 0.5 }, 0.0)?;
                    let visible = bounds[0].floor() < state.bounds[2]
                        && bounds[2].ceil() > state.bounds[0]
                        && bounds[1].floor() < state.bounds[3]
                        && bounds[3].ceil() > state.bounds[1];
                    geometry = Some((transform, bounds));
                    Ok(visible)
                },
            )?
            else {
                continue;
            };
            let (transform, bounds) = geometry.unwrap();
            let [a, b, c, d, e, f] = transform.inverse()?.coefficients();
            let mask = image.content == Content::Mask;
            recording.record(
                Primitive {
                    bounds,
                    row0: [a, c, e, 0.0],
                    row1: [b, d, f, 0.0],
                    rect: image.rect,
                    params: [contrast, 0.0, width as f32, height as f32],
                    color: if mask {
                        linear_rgba(run.color().to_rgba())
                    } else {
                        [f32::from(alpha) / 255.0; 4]
                    },
                    header: [state.clip, if mask { 1 } else { 2 }, image.page, 0],
                },
                state.bounds,
                recording_budget,
            )?;
        }
        Ok(())
    }
}
