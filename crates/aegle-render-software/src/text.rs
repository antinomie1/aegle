use aegle_glyph::{Content, Glyph, RasterOptions, RasterTransform};
use aegle_scene::GlyphRun;
use aegle_types::{Color, Point};

use crate::{
    Frame, RenderError,
    blend::{Solid, encoded_rgba, linear_rgba},
    raster::{State, coverage_product},
    surface::order,
    vector::device_bounds,
};

impl Frame<'_, '_, '_> {
    pub(crate) fn paint_text(&mut self, run: &GlyphRun, state: State) -> Result<(), RenderError> {
        if state.bounds.is_empty() || run.color().to_rgba()[3] == 0 {
            return Ok(());
        }
        let raster =
            RasterTransform::new(state.transform, run.size()).map_err(RenderError::Glyph)?;
        let aligned = raster.hint();
        let solid = Solid::new(self.surface.color(run.color()));
        let bgra = self.surface.bgra;
        let [r, g, blue, opacity] = run.color().to_rgba();
        let contrast = (aegle_glyph::mask_contrast(run.color().to_rgba()) * 255.0).round() as i32;
        // Integer form of the shared monotonic curve: c + c(255 - c)k / 255².
        let adjust = |c: u8| {
            let c = i32::from(c);
            (c + c * (255 - c) * contrast / 65_025) as u8
        };
        for glyph in run.glyphs() {
            let origin = raster
                .origin(glyph.position)
                .map_err(|_| RenderError::Coordinates)?;
            let image = self
                .renderer
                .glyphs
                .rasterize(
                    run.font(),
                    glyph.id,
                    RasterOptions {
                        size: raster.size(),
                        offset: origin.offset(),
                        normalized_coords: run.normalized_coords(),
                        hint: aligned,
                        foreground: [r, g, blue, 255],
                        embolden: run.embolden(),
                        skew: run.skew(),
                    },
                )
                .map_err(RenderError::Glyph)?;
            if image.data.is_empty() {
                continue;
            }
            let transform = origin
                .image_transform(image.placement)
                .map_err(|_| RenderError::Coordinates)?;
            let (width, height) = (image.placement.width, image.placement.height);
            let bounds = device_bounds(width, height, transform, self.surface, !aligned)?
                .intersect(state.bounds);
            let inverse = transform.inverse().map_err(|_| RenderError::Coordinates)?;
            let clip = (state.clips > 0).then(|| self.renderer.masks[state.clips].data());
            let width = self.surface.width as usize;
            for row in bounds.rows(width) {
                let y = (row.start / width) as f32 + 0.5;
                let pixels = &mut self.surface.data[row.start * 4..row.end * 4];
                for (i, pixel) in row.zip(pixels.chunks_exact_mut(4)) {
                    let local = inverse.map_point(Point::new((i % width) as f32 + 0.5, y));
                    let rgba = if aligned {
                        fetch(image, local.x.floor() as i32, local.y.floor() as i32)
                    } else {
                        sample(image, local.x - 0.5, local.y - 0.5)
                    };
                    let coverage = clip.map_or(255, |mask| mask[i]);
                    match image.content {
                        Content::Mask => {
                            solid.blend(pixel, coverage_product(adjust(rgba[3]), coverage))
                        }
                        Content::Color => {
                            let [r, g, b, _] = order(bgra, rgba);
                            Solid::new(Color::rgba(r, g, b, coverage_product(rgba[3], opacity)))
                                .blend(pixel, coverage);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn fetch(image: Glyph<'_>, x: i32, y: i32) -> [u8; 4] {
    if x < 0 || y < 0 || x as u32 >= image.placement.width || y as u32 >= image.placement.height {
        return [0; 4];
    }
    let index = y as usize * image.placement.width as usize + x as usize;
    match image.content {
        Content::Mask => [255, 255, 255, image.data[index]],
        Content::Color => image.data[index * 4..index * 4 + 4].try_into().unwrap(),
    }
}

fn sample(image: Glyph<'_>, x: f32, y: f32) -> [u8; 4] {
    // Inverse transforms can magnify a subpixel footprint to huge coordinates.
    // Cull outside filter support before converting to bounded integer indices.
    if !(x > -1.0
        && y > -1.0
        && x < image.placement.width as f32
        && y < image.placement.height as f32)
    {
        return [0; 4];
    }
    let left = x.floor();
    let top = y.floor();
    let dx = x - left;
    let dy = y - top;
    let mut premul = [0.0; 4];
    for (ox, oy, weight) in [
        (0, 0, (1.0 - dx) * (1.0 - dy)),
        (1, 0, dx * (1.0 - dy)),
        (0, 1, (1.0 - dx) * dy),
        (1, 1, dx * dy),
    ] {
        let rgba = fetch(image, left as i32 + ox, top as i32 + oy);
        if image.content == Content::Mask {
            premul[3] += rgba[3] as f32 / 255.0 * weight;
        } else {
            let linear = linear_rgba(rgba);
            for channel in 0..4 {
                premul[channel] += linear[channel] * weight;
            }
        }
    }
    if image.content == Content::Mask {
        [255, 255, 255, (premul[3] * 255.0).round() as u8]
    } else {
        encoded_rgba(premul)
    }
}
