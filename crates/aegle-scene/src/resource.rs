//! Immutable images and vector outlines shared by scenes and renderer caches.

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{Point, Rect, SceneError};

static NEXT_ID: AtomicUsize = AtomicUsize::new(1);

fn next_id() -> usize {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Shared, immutable unpremultiplied sRGB RGBA8 pixels, top row first.
///
/// Cloning shares the pixels. Each constructed image has a process-unique
/// [`Image::id`], which renderers use to cache uploaded copies.
#[derive(Clone)]
pub struct Image(Arc<ImageData>);

struct ImageData {
    id: usize,
    width: u32,
    height: u32,
    pixels: Box<[u8]>,
}

impl Image {
    /// Largest accepted width or height.
    pub const MAX_EXTENT: u32 = 16_384;

    /// Takes tightly packed pixels; both extents are 1..=[`Self::MAX_EXTENT`].
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, SceneError> {
        let extent = 1..=Self::MAX_EXTENT;
        if !extent.contains(&width)
            || !extent.contains(&height)
            || pixels.len() != width as usize * height as usize * 4
        {
            return Err(SceneError::InvalidImage);
        }
        Ok(Self(Arc::new(ImageData {
            id: next_id(),
            width,
            height,
            pixels: pixels.into_boxed_slice(),
        })))
    }

    /// Cache identity shared by clones of this image.
    pub fn id(&self) -> usize {
        self.0.id
    }
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.0.width
    }
    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.0.height
    }
    /// Unpremultiplied sRGB RGBA8 rows.
    pub fn pixels(&self) -> &[u8] {
        &self.0.pixels
    }
}

impl core::fmt::Debug for Image {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Image#{}({}x{})", self.id(), self.width(), self.height())
    }
}

/// One outline operation; its points follow in [`Path::points`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    /// Starts a contour at one point.
    Move,
    /// Straight segment to one point.
    Line,
    /// Quadratic segment through a control point to an end point.
    Quad,
    /// Cubic segment through two control points to an end point.
    Cubic,
    /// Closes the current contour back to its start.
    Close,
}

impl Verb {
    /// Number of points this operation consumes.
    pub const fn points(self) -> usize {
        match self {
            Self::Move | Self::Line => 1,
            Self::Quad => 2,
            Self::Cubic => 3,
            Self::Close => 0,
        }
    }
}

/// Inside test for filled paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    /// Nonzero winding.
    #[default]
    NonZero,
    /// Alternating inside and outside.
    EvenOdd,
}

/// End shape of open stroked contours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    /// Ends exactly at the end point.
    #[default]
    Butt,
    /// Adds a half-disc.
    Round,
    /// Extends by half the width.
    Square,
}

/// Corner shape between stroked segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    /// Sharp corner, beveled beyond a miter limit of 4.
    #[default]
    Miter,
    /// Rounded corner.
    Round,
    /// Flat corner.
    Bevel,
}

/// Centered path stroke, measured in local units and transformed with the path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Nonnegative width; zero draws nothing.
    pub width: f32,
    /// Open-contour ends.
    pub cap: LineCap,
    /// Segment corners.
    pub join: LineJoin,
}

impl Stroke {
    /// Butt-capped, mitered stroke of `width`.
    pub fn new(width: f32) -> Self {
        Self {
            width,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
        }
    }
}

/// Shared, immutable outline with a fill rule and control-point bounds.
///
/// Cloning shares storage. Each finished path has a process-unique
/// [`Path::id`], which renderers use to cache rasterized coverage.
#[derive(Clone)]
pub struct Path(Arc<PathData>);

struct PathData {
    id: usize,
    verbs: Box<[Verb]>,
    points: Box<[Point]>,
    bounds: Rect,
    rule: FillRule,
}

impl Path {
    /// Cache identity shared by clones of this path.
    pub fn id(&self) -> usize {
        self.0.id
    }
    /// Operations in drawing order.
    pub fn verbs(&self) -> &[Verb] {
        &self.0.verbs
    }
    /// Points consumed in order by [`Self::verbs`].
    pub fn points(&self) -> &[Point] {
        &self.0.points
    }
    /// Bounds of every point, including curve control points.
    pub fn bounds(&self) -> Rect {
        self.0.bounds
    }
    /// Fill rule used by fills.
    pub fn fill_rule(&self) -> FillRule {
        self.0.rule
    }
    /// Whether the outline has no segment.
    pub fn is_empty(&self) -> bool {
        self.0.points.len() == self.0.verbs.iter().filter(|v| **v == Verb::Move).count()
    }
}

impl core::fmt::Debug for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Path#{}({} verbs)", self.id(), self.verbs().len())
    }
}

/// Accumulates an outline; [`Self::finish`] validates it once.
#[derive(Debug, Default)]
pub struct PathBuilder {
    verbs: Vec<Verb>,
    points: Vec<Point>,
}

impl PathBuilder {
    /// Creates an empty builder without allocating.
    pub fn new() -> Self {
        Self::default()
    }
    /// Starts a contour.
    pub fn move_to(&mut self, point: Point) -> &mut Self {
        self.push(Verb::Move, &[point])
    }
    /// Adds a straight segment.
    pub fn line_to(&mut self, point: Point) -> &mut Self {
        self.push(Verb::Line, &[point])
    }
    /// Adds a quadratic segment.
    pub fn quad_to(&mut self, control: Point, point: Point) -> &mut Self {
        self.push(Verb::Quad, &[control, point])
    }
    /// Adds a cubic segment.
    pub fn cubic_to(&mut self, first: Point, second: Point, point: Point) -> &mut Self {
        self.push(Verb::Cubic, &[first, second, point])
    }
    /// Closes the current contour; the next segment needs a new `move_to`.
    pub fn close(&mut self) -> &mut Self {
        self.push(Verb::Close, &[])
    }

    fn push(&mut self, verb: Verb, points: &[Point]) -> &mut Self {
        self.verbs.push(verb);
        self.points.extend_from_slice(points);
        self
    }

    /// Finishes the path. Panics on nonfinite points or a segment that does
    /// not follow `move_to`.
    #[track_caller]
    pub fn finish(self, rule: FillRule) -> Path {
        let mut open = false;
        for verb in &self.verbs {
            match verb {
                Verb::Move => open = true,
                Verb::Close if open => open = false,
                _ if !open => crate::fail(SceneError::InvalidPath),
                _ => {}
            }
        }
        let mut bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for point in &self.points {
            if !point.x.is_finite() || !point.y.is_finite() {
                crate::fail(SceneError::NonFinite);
            }
            bounds = [
                bounds[0].min(point.x),
                bounds[1].min(point.y),
                bounds[2].max(point.x),
                bounds[3].max(point.y),
            ];
        }
        let bounds = if self.points.is_empty() {
            Rect::new(0.0, 0.0, 0.0, 0.0)
        } else {
            Rect::new(
                bounds[0],
                bounds[1],
                bounds[2] - bounds[0],
                bounds[3] - bounds[1],
            )
        };
        if !bounds.size.width.is_finite() || !bounds.size.height.is_finite() {
            crate::fail(SceneError::CoordinateRange);
        }
        Path(Arc::new(PathData {
            id: next_id(),
            verbs: self.verbs.into_boxed_slice(),
            points: self.points.into_boxed_slice(),
            bounds,
            rule,
        }))
    }
}
