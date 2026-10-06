use aegle_glyph::{Content, RasterOptions, RasterTransform};
use aegle_gpu::{Recording, State, Textured, bounds};
use aegle_scene::{Command, GlyphRun, Rect, RoundedRect, Scene};
use aegle_types::color_math::linear_rgba;

use crate::{
    Error, Result, TextOptions, atlas::Atlas, device::Device, pipeline::Pipeline,
    text_pipeline::TextPipeline,
};

pub(crate) struct Text {
    // Descriptor views are released before the descriptor pool and sampler.
    pub atlas: Atlas,
    pub pipeline: TextPipeline,
    /// Reused path-mask rasterizer storage.
    pub scratch: aegle_gpu::Scratch,
}

/// Shader viewport parameters and remaining device budget for atlas-backed draws.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub viewport: [f32; 2],
    pub device: u64,
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
            scratch: aegle_gpu::Scratch::new(),
        })
    }

    /// Records one glyph, image or path command through the atlas.
    pub fn record(
        &mut self,
        device: &Device,
        recording: &mut Recording,
        scene: &Scene,
        command: Command,
        state: State,
        limits: Limits,
    ) -> Result {
        match command {
            Command::Glyphs(index) => {
                self.glyphs(device, recording, &scene.glyph_runs()[index], state, limits)
            }
            Command::Image { image, rect } => self.image(
                device,
                recording,
                &scene.images()[image],
                rect,
                state,
                limits,
            ),
            Command::FillPath { path, color } => {
                let path = &scene.paths()[path];
                self.path(device, recording, path, color, None, state, limits)
            }
            Command::StrokePath {
                path,
                color,
                stroke,
            } => {
                let path = &scene.paths()[path];
                self.path(device, recording, path, color, Some(stroke), state, limits)
            }
            _ => unreachable!("geometry commands are recorded without the atlas"),
        }
    }

    fn glyphs(
        &mut self,
        device: &Device,
        recording: &mut Recording,
        run: &GlyphRun,
        state: State,
        limits: Limits,
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
                    embolden: run.embolden(),
                    skew: run.skew(),
                },
                device,
                &self.pipeline,
                limits.device,
                |placement| {
                    let transform = origin.image_transform(placement)?;
                    let shape = RoundedRect::new(
                        Rect::new(0.0, 0.0, placement.width as f32, placement.height as f32),
                        0.0,
                    )?;
                    // Bitmap filtering already contributes its half-pixel support;
                    // the analytic geometry AA fringe must not pin invisible glyphs.
                    let area =
                        bounds(shape, transform, if raster.hint() { 0.0 } else { 0.5 }, 0.0)?;
                    geometry = Some((transform, area));
                    Ok(aegle_gpu::visible(area, state.bounds))
                },
            )?
            else {
                continue;
            };
            let (transform, area) = geometry.unwrap();
            let mask = image.content == Content::Mask;
            recording.record(
                Textured {
                    area,
                    inverse: transform.inverse()?,
                    rect: image.rect,
                    contrast,
                    viewport: limits.viewport,
                    color: if mask {
                        linear_rgba(run.color().to_rgba())
                    } else {
                        [f32::from(alpha) / 255.0; 4]
                    },
                    kind: if mask { 1 } else { 2 },
                    page: image.page,
                }
                .primitive(state.clip),
                state.bounds,
            )?;
        }
        Ok(())
    }
}
