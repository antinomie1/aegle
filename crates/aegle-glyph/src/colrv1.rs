//! COLRv1 paint graphs rendered into one straight sRGB RGBA8 bitmap with tiny-skia.
//!
//! Gradients, clips, transforms and composite layers all resolve into a single
//! image, so every renderer draws these glyphs through its ordinary color path.
//! Sweep gradients, which tiny-skia lacks, are evaluated per pixel here.
use skrifa::{
    GlyphId,
    color::{Brush, ColorGlyph, ColorPainter, ColorStop, CompositeMode, Extend, Transform},
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlineGlyphCollection, OutlinePen},
    raw::types::{BoundingBox, F2Dot14},
};
use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder,
    Pixmap, PixmapPaint, Point, RadialGradient, Rect, Shader, SpreadMode, Transform as Matrix,
};

use crate::{Content, GlyphError, Image, Placement, raster::image_size};

/// Deepest nesting of clips or layers a glyph may use; deeper graphs are refused.
const DEPTH: usize = 32;

pub(crate) struct Request<'a> {
    pub glyph: ColorGlyph<'a>,
    pub outlines: OutlineGlyphCollection<'a>,
    pub coords: &'a [i16],
    pub units_per_em: f32,
    pub size: f32,
    pub offset: [f32; 2],
    pub palette: Vec<[u8; 4]>,
    pub foreground: [u8; 4],
    pub max_bytes: usize,
}

pub(crate) fn render(
    request: Request<'_>,
    mut reserve: impl FnMut(usize) -> Result<(), GlyphError>,
) -> Result<Image, GlyphError> {
    let coords: Vec<F2Dot14> = request
        .coords
        .iter()
        .map(|c| F2Dot14::from_bits(*c))
        .collect();
    let location = LocationRef::new(&coords);
    let scale = request.size / request.units_per_em;
    // Without a ClipList the extent is unknown; assume a generous 2 em square.
    let bounds = request
        .glyph
        .bounding_box(location, Size::unscaled())
        .unwrap_or(BoundingBox {
            x_min: -0.5 * request.units_per_em,
            y_min: -0.5 * request.units_per_em,
            x_max: 1.5 * request.units_per_em,
            y_max: 1.5 * request.units_per_em,
        });
    let [ox, oy] = request.offset;
    let left = (bounds.x_min * scale + ox).floor();
    let top = (-bounds.y_max * scale + oy).floor();
    let width = ((bounds.x_max * scale + ox).ceil() - left).max(0.0);
    let height = ((-bounds.y_min * scale + oy).ceil() - top).max(0.0);
    if !(width < 65_536.0 && height < 65_536.0) {
        return Err(GlyphError::ImageBudget);
    }
    let (width, height) = (width as u32, height as u32);
    let bytes = image_size(width, height, 4, request.max_bytes)?;
    reserve(bytes)?;
    let mut painter = Painter {
        outlines: request.outlines,
        location,
        palette: request.palette,
        foreground: request.foreground,
        transforms: vec![Matrix::from_row(
            scale,
            0.0,
            0.0,
            -scale,
            ox - left,
            oy - top,
        )],
        clips: Vec::new(),
        layers: vec![Pixmap::new(width, height).ok_or(GlyphError::ImageBudget)?],
        modes: Vec::new(),
        failure: None,
    };
    request
        .glyph
        .paint(location, &mut painter)
        .map_err(|_| GlyphError::InvalidFont)?;
    if let Some(error) = painter.failure {
        return Err(error);
    }
    let pixmap = painter.layers.swap_remove(0);
    Ok(Image {
        placement: Placement {
            left: left as i32,
            top: -(top as i32),
            width,
            height,
        },
        content: Content::Color,
        data: pixmap.take_demultiplied().into_boxed_slice(),
    })
}

struct Painter<'a> {
    outlines: OutlineGlyphCollection<'a>,
    location: LocationRef<'a>,
    palette: Vec<[u8; 4]>,
    foreground: [u8; 4],
    /// Font units to bitmap pixels, then each pushed paint transform.
    transforms: Vec<Matrix>,
    clips: Vec<Mask>,
    layers: Vec<Pixmap>,
    modes: Vec<CompositeMode>,
    failure: Option<GlyphError>,
}

struct Pen(PathBuilder);

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, cy0, x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, cy0, cx1, cy1, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

impl Painter<'_> {
    fn current(&self) -> Matrix {
        *self.transforms.last().expect("base transform")
    }

    fn fail(&mut self, error: GlyphError) {
        self.failure.get_or_insert(error);
    }

    fn glyph_path(&mut self, glyph: GlyphId) -> Option<Path> {
        let outline = self.outlines.get(glyph)?;
        let mut pen = Pen(PathBuilder::new());
        outline
            .draw(
                DrawSettings::unhinted(Size::unscaled(), self.location),
                &mut pen,
            )
            .ok()?;
        pen.0.finish()
    }

    fn clip(&mut self, path: Option<Path>) {
        if self.clips.len() >= DEPTH {
            return self.fail(GlyphError::ImageBudget);
        }
        let layer = self.layers.last().expect("layer");
        let mut mask = match self.clips.last() {
            Some(previous) => previous.clone(),
            None => {
                // An empty path clips everything; start fully open instead for
                // clip boxes and let the intersection narrow it.
                let Some(mut open) = Mask::new(layer.width(), layer.height()) else {
                    return self.fail(GlyphError::ImageBudget);
                };
                open.fill_path(
                    &PathBuilder::from_rect(
                        Rect::from_xywh(0.0, 0.0, layer.width() as f32, layer.height() as f32)
                            .expect("positive extent"),
                    ),
                    FillRule::Winding,
                    false,
                    Matrix::identity(),
                );
                open
            }
        };
        match path {
            Some(path) => mask.intersect_path(&path, FillRule::Winding, true, self.current()),
            // A missing outline paints nothing.
            None => mask.clear(),
        }
        self.clips.push(mask);
    }

    fn color(&self, index: u16, alpha: f32) -> Color {
        let [r, g, b, a] = if index == 0xFFFF {
            self.foreground
        } else {
            self.palette
                .get(usize::from(index))
                .copied()
                .unwrap_or(self.foreground)
        };
        Color::from_rgba8(
            r,
            g,
            b,
            (f32::from(a) * alpha.clamp(0.0, 1.0)).round() as u8,
        )
    }

    fn stops(&self, stops: &[ColorStop]) -> Vec<(f32, Color)> {
        stops
            .iter()
            .map(|s| {
                (
                    s.offset.clamp(0.0, 1.0),
                    self.color(s.palette_index, s.alpha),
                )
            })
            .collect()
    }

    fn sweep(
        &mut self,
        center: skrifa::raw::types::Point<f32>,
        start: f32,
        end: f32,
        stops: &[ColorStop],
        extend: Extend,
    ) {
        let transform = self.current();
        let Some(inverse) = transform.invert() else {
            return;
        };
        let stops = self.stops(stops);
        let (width, height) = {
            let layer = self.layers.last().expect("layer");
            (layer.width() as usize, layer.height() as usize)
        };
        let coverage = self.clips.last().map(|mask| mask.data().to_vec());
        let layer = self.layers.last_mut().expect("layer");
        let span = (end - start).max(f32::EPSILON);
        for y in 0..height {
            for x in 0..width {
                let cover = coverage.as_ref().map_or(255, |c| c[y * width + x]);
                if cover == 0 {
                    continue;
                }
                let mut point = Point::from_xy(x as f32 + 0.5, y as f32 + 0.5);
                inverse.map_points(std::slice::from_mut(&mut point));
                // Skrifa reports clockwise angles in font space (y up) and reverses
                // the stops, which restores the font file's counter-clockwise sweep.
                let degrees = (-(point.y - center.y))
                    .atan2(point.x - center.x)
                    .to_degrees();
                let t = (degrees.rem_euclid(360.0) - start) / span;
                let color = ramp(&stops, spread(t, extend));
                let alpha = color.alpha() * f32::from(cover) / 255.0;
                let source = [color.red(), color.green(), color.blue()].map(|c| c * alpha);
                let at = (y * width + x) * 4;
                let pixel = &mut layer.data_mut()[at..at + 4];
                for (channel, value) in pixel
                    .iter_mut()
                    .zip([source[0], source[1], source[2], alpha])
                {
                    let below = f32::from(*channel) / 255.0;
                    *channel = ((value + below * (1.0 - alpha)) * 255.0).round() as u8;
                }
            }
        }
    }
}

fn spread(t: f32, extend: Extend) -> f32 {
    match extend {
        Extend::Repeat => t.rem_euclid(1.0),
        Extend::Reflect => 1.0 - (t.rem_euclid(2.0) - 1.0).abs(),
        _ => t.clamp(0.0, 1.0),
    }
}

/// Linear interpolation between neighboring stops, clamped at both ends.
fn ramp(stops: &[(f32, Color)], t: f32) -> Color {
    let (first, last) = (stops[0], stops[stops.len() - 1]);
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let ((start, a), (end, b)) = (pair[0], pair[1]);
        if t <= end {
            let f = (t - start) / (end - start).max(f32::EPSILON);
            let mix = |x: f32, y: f32| x + (y - x) * f;
            return Color::from_rgba(
                mix(a.red(), b.red()),
                mix(a.green(), b.green()),
                mix(a.blue(), b.blue()),
                mix(a.alpha(), b.alpha()),
            )
            .unwrap_or(last.1);
        }
    }
    last.1
}

fn gradient_stops(stops: Vec<(f32, Color)>) -> Vec<GradientStop> {
    stops
        .into_iter()
        .map(|(offset, color)| GradientStop::new(offset, color))
        .collect()
}

fn mode(mode: CompositeMode) -> BlendMode {
    use CompositeMode as C;
    match mode {
        C::Clear => BlendMode::Clear,
        C::Src => BlendMode::Source,
        C::Dest => BlendMode::Destination,
        C::DestOver => BlendMode::DestinationOver,
        C::SrcIn => BlendMode::SourceIn,
        C::DestIn => BlendMode::DestinationIn,
        C::SrcOut => BlendMode::SourceOut,
        C::DestOut => BlendMode::DestinationOut,
        C::SrcAtop => BlendMode::SourceAtop,
        C::DestAtop => BlendMode::DestinationAtop,
        C::Xor => BlendMode::Xor,
        C::Plus => BlendMode::Plus,
        C::Screen => BlendMode::Screen,
        C::Overlay => BlendMode::Overlay,
        C::Darken => BlendMode::Darken,
        C::Lighten => BlendMode::Lighten,
        C::ColorDodge => BlendMode::ColorDodge,
        C::ColorBurn => BlendMode::ColorBurn,
        C::HardLight => BlendMode::HardLight,
        C::SoftLight => BlendMode::SoftLight,
        C::Difference => BlendMode::Difference,
        C::Exclusion => BlendMode::Exclusion,
        C::Multiply => BlendMode::Multiply,
        C::HslHue => BlendMode::Hue,
        C::HslSaturation => BlendMode::Saturation,
        C::HslColor => BlendMode::Color,
        C::HslLuminosity => BlendMode::Luminosity,
        _ => BlendMode::SourceOver,
    }
}

fn spread_mode(extend: Extend) -> SpreadMode {
    match extend {
        Extend::Repeat => SpreadMode::Repeat,
        Extend::Reflect => SpreadMode::Reflect,
        _ => SpreadMode::Pad,
    }
}

impl ColorPainter for Painter<'_> {
    fn push_transform(&mut self, transform: Transform) {
        if self.transforms.len() > DEPTH * 4 {
            return self.fail(GlyphError::ImageBudget);
        }
        let t = Matrix::from_row(
            transform.xx,
            transform.yx,
            transform.xy,
            transform.yy,
            transform.dx,
            transform.dy,
        );
        self.transforms.push(self.current().pre_concat(t));
    }

    fn pop_transform(&mut self) {
        self.transforms.pop();
    }

    fn push_clip_glyph(&mut self, glyph: GlyphId) {
        let path = self.glyph_path(glyph);
        self.clip(path);
    }

    fn push_clip_box(&mut self, clip: BoundingBox<f32>) {
        let path = Rect::from_ltrb(clip.x_min, clip.y_min, clip.x_max, clip.y_max)
            .map(PathBuilder::from_rect);
        self.clip(path);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn fill(&mut self, brush: Brush<'_>) {
        let transform = self.current();
        let shader = match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => Shader::SolidColor(self.color(palette_index, alpha)),
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let stops = self.stops(color_stops);
                if stops.len() == 1 {
                    Shader::SolidColor(stops[0].1)
                } else {
                    let Some(shader) = LinearGradient::new(
                        Point::from_xy(p0.x, p0.y),
                        Point::from_xy(p1.x, p1.y),
                        gradient_stops(stops),
                        spread_mode(extend),
                        transform,
                    ) else {
                        return;
                    };
                    shader
                }
            }
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let stops = self.stops(color_stops);
                if stops.len() == 1 {
                    Shader::SolidColor(stops[0].1)
                } else {
                    let Some(shader) = RadialGradient::new(
                        Point::from_xy(c0.x, c0.y),
                        r0.max(0.0),
                        Point::from_xy(c1.x, c1.y),
                        r1.max(0.0),
                        gradient_stops(stops),
                        spread_mode(extend),
                        transform,
                    ) else {
                        return;
                    };
                    shader
                }
            }
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => return self.sweep(c0, start_angle, end_angle, color_stops, extend),
        };
        let paint = Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        };
        let layer = self.layers.last_mut().expect("layer");
        let everything = Rect::from_xywh(0.0, 0.0, layer.width() as f32, layer.height() as f32)
            .expect("positive extent");
        layer.fill_rect(everything, &paint, Matrix::identity(), self.clips.last());
    }

    fn push_layer(&mut self, composite: CompositeMode) {
        let below = self.layers.last().expect("layer");
        let Some(layer) = Pixmap::new(below.width(), below.height()) else {
            return self.fail(GlyphError::ImageBudget);
        };
        if self.layers.len() >= DEPTH {
            return self.fail(GlyphError::ImageBudget);
        }
        self.layers.push(layer);
        self.modes.push(composite);
    }

    fn pop_layer(&mut self) {
        let (Some(top), Some(composite)) = (self.layers.pop(), self.modes.pop()) else {
            return;
        };
        let paint = PixmapPaint {
            blend_mode: mode(composite),
            ..PixmapPaint::default()
        };
        self.layers.last_mut().expect("layer").draw_pixmap(
            0,
            0,
            top.as_ref(),
            &paint,
            Matrix::identity(),
            None,
        );
    }
}
