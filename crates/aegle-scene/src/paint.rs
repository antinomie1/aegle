//! Gradient paints filling shapes, shared by scenes and renderers.

use alloc::sync::Arc;

use crate::{Color, Point, SceneError};

/// A color at an offset along a gradient, `0.0` first to `1.0` last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position in `0..=1`.
    pub offset: f32,
    /// Unpremultiplied sRGB color.
    pub color: Color,
}

/// Where a gradient's first stop runs to its last, in the local coordinates of
/// the shape it fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientGeometry {
    /// Along the line from `start` to `end`, constant across it.
    Linear {
        /// Position of offset 0.
        start: Point,
        /// Position of offset 1.
        end: Point,
    },
    /// Outward from `center` to the circle of `radius`.
    Radial {
        /// Position of offset 0.
        center: Point,
        /// Positive distance of offset 1.
        radius: f32,
    },
}

/// A validated gradient: 2..=[`Gradient::MAX_STOPS`] stops with
/// nondecreasing offsets in `0..=1`, padded with the end colors beyond them.
///
/// Renderers interpolate between stops in premultiplied linear light, like
/// motion and image effects. Cloning shares the stops.
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    geometry: GradientGeometry,
    stops: Arc<[GradientStop]>,
}

impl Gradient {
    /// Largest accepted number of stops.
    pub const MAX_STOPS: usize = 16;

    /// A linear gradient; `start` and `end` must differ.
    pub fn linear(start: Point, end: Point, stops: &[GradientStop]) -> Result<Self, SceneError> {
        Self::new(GradientGeometry::Linear { start, end }, stops)
    }

    /// A circular gradient with a positive `radius`.
    pub fn radial(center: Point, radius: f32, stops: &[GradientStop]) -> Result<Self, SceneError> {
        Self::new(GradientGeometry::Radial { center, radius }, stops)
    }

    fn new(geometry: GradientGeometry, stops: &[GradientStop]) -> Result<Self, SceneError> {
        check(geometry)?;
        let ordered = stops
            .windows(2)
            .all(|pair| pair[0].offset <= pair[1].offset);
        if !(2..=Self::MAX_STOPS).contains(&stops.len())
            || !ordered
            || !stops.iter().all(|stop| (0.0..=1.0).contains(&stop.offset))
        {
            return Err(SceneError::InvalidGradient);
        }
        Ok(Self {
            geometry,
            stops: stops.into(),
        })
    }

    /// The same stops, shared, along another validated `geometry`.
    pub fn with_geometry(&self, geometry: GradientGeometry) -> Result<Self, SceneError> {
        check(geometry)?;
        Ok(Self {
            geometry,
            stops: self.stops.clone(),
        })
    }

    /// Its geometry in local coordinates.
    pub fn geometry(&self) -> GradientGeometry {
        self.geometry
    }

    /// Its stops, in order.
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }
}

fn check(geometry: GradientGeometry) -> Result<(), SceneError> {
    let (values, degenerate) = match geometry {
        GradientGeometry::Linear { start, end } => ([start.x, start.y, end.x, end.y], start == end),
        GradientGeometry::Radial { center, radius } => {
            ([center.x, center.y, radius, 0.0], radius <= 0.0)
        }
    };
    if !values.into_iter().all(f32::is_finite) {
        return Err(SceneError::NonFinite);
    }
    if degenerate {
        return Err(SceneError::InvalidGradient);
    }
    Ok(())
}
