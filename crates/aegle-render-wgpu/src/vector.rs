//! Images and CPU-rasterized path coverage drawn from the shared atlas.
//!
//! Path masks are cached per path, linear transform, quarter-pixel phase and
//! stroke style, so moving a path by whole pixels reuses its uploaded mask.
use aegle_glyph::{Content, Glyph, Placement};
use aegle_scene::{
    Affine, Color, FillRule, Image, LineCap, LineJoin, Path, Rect, RoundedRect, Stroke, Verb,
};
use aegle_types::color_math::linear_rgba;
use zeno::{Command as Segment, Mask, Point as ZenoPoint};

use crate::{
    Error, Renderer, Result,
    atlas::{AtlasGlyph, ResourceKey},
    records::{Kind, Primitive, State, bounds},
};

impl Renderer {
    pub(crate) fn image(&mut self, image: &Image, rect: Rect, state: State) -> Result {
        let (width, height) = (image.width(), image.height());
        let transform = Affine::new([
            rect.size.width / width as f32,
            0.0,
            0.0,
            rect.size.height / height as f32,
            rect.origin.x,
            rect.origin.y,
        ])
        .and_then(|local| local.then(state.transform))
        .map_err(|_| Error::Coordinates)?;
        let shape = RoundedRect::new(Rect::new(0.0, 0.0, width as f32, height as f32), 0.0)?;
        // Edge coverage antialiases within one device pixel of the image rect.
        let area = bounds(shape, transform, 0.0, 1.0)?;
        if !visible(area, state.bounds) {
            return Ok(());
        }
        let key = ResourceKey {
            id: image.id(),
            params: [0; 7],
        };
        let entry = match self.atlas.resource(key) {
            Some(entry) => entry,
            None => self.place(
                key,
                Glyph {
                    placement: Placement {
                        left: 0,
                        top: 0,
                        width,
                        height,
                    },
                    content: Content::Color,
                    data: image.pixels(),
                },
            )?,
        };
        let inverse = transform.inverse().map_err(|_| Error::Coordinates)?;
        self.emit(entry, area, inverse, [1.0; 4], 3, state)
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
        // Miter joins reach at most twice the width (limit 4) from the outline.
        let outset = stroke.map_or(0.0, |stroke| stroke.width * 2.0);
        let hull = RoundedRect::new(path.bounds(), 0.0)?;
        if !visible(bounds(hull, state.transform, outset, 1.0)?, state.bounds) {
            return Ok(());
        }
        let [a, b, c, d, e, f] = state.transform.coefficients();
        // Whole-pixel translation stays outside the mask; four phases per axis.
        let quarter = |value: f32| {
            let steps = (value * 4.0).round();
            let whole = (steps / 4.0).floor();
            (whole, steps - whole * 4.0)
        };
        let ((x, phase_x), (y, phase_y)) = (quarter(e), quarter(f));
        let style = stroke.map_or(0, |stroke| {
            1 | (stroke.cap as u32) << 1 | (stroke.join as u32) << 3
        });
        let key = ResourceKey {
            id: path.id(),
            params: [
                a.to_bits(),
                b.to_bits(),
                c.to_bits(),
                d.to_bits(),
                phase_x as u32 | (phase_y as u32) << 2,
                style,
                stroke.map_or(0, |stroke| stroke.width.to_bits()),
            ],
        };
        let entry = match self.atlas.resource(key) {
            Some(entry) => entry,
            None => {
                let segments = segments(path);
                let mut mask = Mask::with_scratch(segments.as_slice(), &mut self.atlas.scratch);
                mask.transform(Some(zeno::Transform::new(
                    a,
                    b,
                    c,
                    d,
                    phase_x / 4.0,
                    phase_y / 4.0,
                )));
                match stroke {
                    None => mask.style(match path.fill_rule() {
                        FillRule::NonZero => zeno::Fill::NonZero,
                        FillRule::EvenOdd => zeno::Fill::EvenOdd,
                    }),
                    Some(stroke) => mask.style(
                        *zeno::Stroke::new(stroke.width)
                            .cap(match stroke.cap {
                                LineCap::Butt => zeno::Cap::Butt,
                                LineCap::Round => zeno::Cap::Round,
                                LineCap::Square => zeno::Cap::Square,
                            })
                            .join(match stroke.join {
                                LineJoin::Miter => zeno::Join::Miter,
                                LineJoin::Round => zeno::Join::Round,
                                LineJoin::Bevel => zeno::Join::Bevel,
                            })
                            .miter_limit(4.0),
                    ),
                };
                let (data, placement) = mask.render();
                if placement.width == 0 || placement.height == 0 {
                    return Ok(());
                }
                self.place(
                    key,
                    Glyph {
                        placement,
                        content: Content::Mask,
                        data: &data,
                    },
                )?
            }
        };
        let left = x + entry.placement.left as f32;
        let top = y + entry.placement.top as f32;
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
        let [a, b, c, d, e, f] = inverse.coefficients();
        let [width, height] = self.size.map(|v| v as f32);
        self.rec.record(
            Primitive {
                bounds: area,
                row0: [a, c, e, 0.0],
                row1: [b, d, f, 0.0],
                rect: entry.rect,
                params: [0.0, 0.0, width, height],
                color,
                header: [state.clip, kind, 0, 0],
            },
            state.bounds,
            Kind::Atlas(entry.slot),
        );
        self.flush_full()
    }
}

fn visible(area: [f32; 4], clip: [f32; 4]) -> bool {
    area[0].floor() < clip[2]
        && area[2].ceil() > clip[0]
        && area[1].floor() < clip[3]
        && area[3].ceil() > clip[1]
}

fn segments(path: &Path) -> Vec<Segment> {
    let mut points = path.points().iter().map(|p| ZenoPoint::new(p.x, p.y));
    let mut next = || points.next().unwrap();
    path.verbs()
        .iter()
        .map(|verb| match verb {
            Verb::Move => Segment::MoveTo(next()),
            Verb::Line => Segment::LineTo(next()),
            Verb::Quad => Segment::QuadTo(next(), next()),
            Verb::Cubic => Segment::CurveTo(next(), next(), next()),
            Verb::Close => Segment::Close,
        })
        .collect()
}
