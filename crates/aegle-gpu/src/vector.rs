//! Image placement and CPU path-mask rasterization shared by atlas renderers.
//!
//! Path masks are keyed by path, linear transform, quarter-pixel phase and
//! stroke style, so moving a path by whole pixels reuses its uploaded mask.
use aegle_scene::{
    Affine, FillRule, Image, LineCap, LineJoin, Path, Rect, RoundedRect, Stroke, Verb,
};
use zeno::{Command as Segment, Mask, Placement, Point as ZenoPoint};

use crate::{Error, Result, State, bounds, visible};

/// Identity of an image or path mask: a scene resource id and raster settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceKey {
    /// Scene image or path id.
    pub id: usize,
    /// Quantized transform, phase and stroke style; all zero for images.
    pub params: [u32; 7],
}

/// Where an image lands on the device.
pub struct ImagePlacement {
    /// Cache identity.
    pub key: ResourceKey,
    /// Device-space quad, widened by the antialiasing fringe.
    pub area: [f32; 4],
    /// Device-to-texel affine.
    pub inverse: Affine,
}

/// Places `image` stretched over `rect`, or `None` when it cannot reach `state`'s
/// clip bounds.
pub fn image_placement(image: &Image, rect: Rect, state: State) -> Result<Option<ImagePlacement>> {
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
        return Ok(None);
    }
    Ok(Some(ImagePlacement {
        key: ResourceKey {
            id: image.id(),
            params: [0; 7],
        },
        area,
        inverse: transform.inverse().map_err(|_| Error::Coordinates)?,
    }))
}

/// A path mask request after quantizing its transform.
pub struct PathRaster {
    /// Cache identity.
    pub key: ResourceKey,
    /// Whole-pixel translation applied outside the mask.
    pub origin: [f32; 2],
    /// Quarter-pixel phases per axis, 0..4, baked into the mask.
    phase: [f32; 2],
    stroke: Option<Stroke>,
}

/// Quantizes `path`'s placement, or `None` when it cannot reach the clip bounds.
pub fn path_raster(
    path: &Path,
    stroke: Option<Stroke>,
    state: State,
) -> Result<Option<PathRaster>> {
    // Miter joins reach at most twice the width (limit 4) from the outline.
    let outset = stroke.map_or(0.0, |stroke| stroke.width * 2.0);
    let hull = RoundedRect::new(path.bounds(), 0.0)?;
    if !visible(bounds(hull, state.transform, outset, 1.0)?, state.bounds) {
        return Ok(None);
    }
    let [a, b, c, d, e, f] = state.transform.coefficients();
    let quarter = |value: f32| {
        let steps = (value * 4.0).round();
        let whole = (steps / 4.0).floor();
        (whole, steps - whole * 4.0)
    };
    let ((x, phase_x), (y, phase_y)) = (quarter(e), quarter(f));
    let style = stroke.map_or(0, |stroke| {
        1 | (stroke.cap as u32) << 1 | (stroke.join as u32) << 3
    });
    Ok(Some(PathRaster {
        key: ResourceKey {
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
        },
        origin: [x, y],
        phase: [phase_x, phase_y],
        stroke,
    }))
}

/// Rasterizes the mask for `raster` under the linear part of `transform`.
/// Returns coverage bytes and their placement; empty masks have zero extent.
pub fn rasterize_path(
    path: &Path,
    raster: &PathRaster,
    transform: Affine,
    scratch: &mut zeno::Scratch,
) -> (Vec<u8>, Placement) {
    let [a, b, c, d, _, _] = transform.coefficients();
    let segments = segments(path);
    let mut mask = Mask::with_scratch(segments.as_slice(), scratch);
    mask.transform(Some(zeno::Transform::new(
        a,
        b,
        c,
        d,
        raster.phase[0] / 4.0,
        raster.phase[1] / 4.0,
    )));
    match raster.stroke {
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
    mask.render()
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
