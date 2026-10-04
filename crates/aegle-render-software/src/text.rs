use aegle_glyph::{Content, Glyph, RasterOptions};
use aegle_scene::{Affine, GlyphRun};
use aegle_types::{Color, Point};

use crate::{
    Frame, RenderError, Surface,
    blend::{Solid, encoded_rgba, linear_rgba},
    raster::{Bounds, State, coverage_product},
};

impl Frame<'_, '_, '_> {
    pub(crate) fn paint_text(&mut self, run: &GlyphRun, state: State) -> Result<(), RenderError> {
        if state.bounds.is_empty() || run.color().to_rgba()[3] == 0 {
            return Ok(());
        }
        let [a, b, c, d, _, _] = state.transform.coefficients();
        let scale = a.hypot(b).max(c.hypot(d));
        let aligned = b == 0.0 && c == 0.0 && a == d && a > 0.0;
        let solid = Solid::new(run.color());
        let [r, g, blue, opacity] = run.color().to_rgba();
        for glyph in run.glyphs() {
            let origin = state.transform.map_point(glyph.position);
            if !origin.x.is_finite()
                || !origin.y.is_finite()
                || origin.x.abs() > 1_048_576.0
                || origin.y.abs() > 1_048_576.0
            {
                return Err(RenderError::Coordinates);
            }
            // Four horizontal/vertical phases bound cache churn while translating
            // text. Glyph advances remain unrounded; only the raster origin snaps.
            let origin = if aligned {
                Point::new(
                    (origin.x * 4.0).round() * 0.25,
                    (origin.y * 4.0).round() * 0.25,
                )
            } else {
                origin
            };
            let offset = if aligned {
                [origin.x - origin.x.floor(), origin.y - origin.y.floor()]
            } else {
                [0.0; 2]
            };
            let image = self
                .renderer
                .glyphs
                .rasterize(
                    run.font(),
                    glyph.id,
                    RasterOptions {
                        size: run.size() * scale,
                        offset,
                        normalized_coords: run.normalized_coords(),
                        hint: aligned,
                        foreground: [r, g, blue, 255],
                    },
                )
                .map_err(RenderError::Glyph)?;
            if image.data.is_empty() {
                continue;
            }
            let transform = if aligned {
                Affine::translation(
                    origin.x.floor() + image.placement.left as f32,
                    origin.y.floor() - image.placement.top as f32,
                )
            } else {
                let left = image.placement.left as f32;
                let top = -(image.placement.top as f32);
                Affine::new([
                    a / scale,
                    b / scale,
                    c / scale,
                    d / scale,
                    origin.x + (a * left + c * top) / scale,
                    origin.y + (b * left + d * top) / scale,
                ])
            }
            .map_err(|_| RenderError::Coordinates)?;
            let bounds =
                device_bounds(image, transform, self.surface, !aligned)?.intersect(state.bounds);
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
                        Content::Mask => solid.blend(pixel, coverage_product(rgba[3], coverage)),
                        Content::Color => {
                            Solid::new(Color::rgba(
                                rgba[0],
                                rgba[1],
                                rgba[2],
                                coverage_product(rgba[3], opacity),
                            ))
                            .blend(pixel, coverage);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn device_bounds(
    image: Glyph<'_>,
    transform: Affine,
    surface: &Surface<'_>,
    filtered: bool,
) -> Result<Bounds, RenderError> {
    let w = image.placement.width as f32;
    let h = image.placement.height as f32;
    let outset = if filtered { 0.5 } else { 0.0 };
    let corners = [
        Point::new(-outset, -outset),
        Point::new(w + outset, -outset),
        Point::new(-outset, h + outset),
        Point::new(w + outset, h + outset),
    ]
    .map(|point| transform.map_point(point));
    if corners.iter().any(|p| {
        !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 1_048_576.0 || p.y.abs() > 1_048_576.0
    }) {
        return Err(RenderError::Coordinates);
    }
    let x0 = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let y0 = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let x1 = corners
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let y1 = corners
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max);
    Ok(Bounds {
        left: x0.floor().clamp(0.0, surface.width as f32) as usize,
        top: y0.floor().clamp(0.0, surface.height as f32) as usize,
        right: x1.ceil().clamp(0.0, surface.width as f32) as usize,
        bottom: y1.ceil().clamp(0.0, surface.height as f32) as usize,
    })
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
