//! Offscreen groups: draws composited as one at an opacity, optionally over a
//! blurred backdrop, and the blur approximation every renderer shares.

use crate::{Affine, Rect, RoundedRect, SceneError};

/// A group of draws composited as one.
///
/// Renderers draw everything between their `push_layer` and `pop_layer` into
/// an offscreen target covering [`Layer::extent`], then blend it source-over
/// onto what lies beneath at [`Layer::opacity`], so overlapping draws inside
/// do not show through each other. A positive backdrop blur first draws the
/// blur of what lies beneath over it, inside [`Layer::shape`]; outside the
/// shape the backdrop is unchanged. Both the composite and the blur are
/// limited to the optional clip. Extent and clip are in the coordinates the
/// renderer draws in (device pixels); the shape is in local coordinates
/// mapped by [`Layer::transform`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    shape: RoundedRect,
    transform: Affine,
    extent: Rect,
    clip: Option<Rect>,
    opacity: f32,
    blur: f32,
}

impl Layer {
    /// Validates a layer: `extent` bounds its content (draws outside it are
    /// cut), `opacity` lies in `0..=1`, and `backdrop_blur` is a nonnegative
    /// Gaussian standard deviation in the shape's local units, zero for none.
    pub fn new(
        shape: RoundedRect,
        transform: Affine,
        extent: Rect,
        clip: Option<Rect>,
        opacity: f32,
        backdrop_blur: f32,
    ) -> Result<Self, SceneError> {
        for rect in [Some(extent), clip].into_iter().flatten() {
            RoundedRect::new(rect, 0.0)?;
        }
        transform.validate_shape(shape, 0.0)?;
        if !(opacity.is_finite() && backdrop_blur.is_finite()) {
            return Err(SceneError::NonFinite);
        }
        if !(0.0..=1.0).contains(&opacity) || backdrop_blur < 0.0 {
            return Err(SceneError::NegativeExtent);
        }
        Ok(Self {
            shape,
            transform,
            extent,
            clip,
            opacity,
            blur: backdrop_blur,
        })
    }

    /// The layer mapped by `outer`, for example a host's device scale; the
    /// extent and clip become the bounding boxes of their mapped corners.
    pub fn then(self, outer: Affine) -> Result<Self, SceneError> {
        Self::new(
            self.shape,
            self.transform.then(outer)?,
            outer.bounds(self.extent, 0.0),
            self.clip.map(|clip| outer.bounds(clip, 0.0)),
            self.opacity,
            self.blur,
        )
    }

    /// The same layer with another clip, such as one snapped to whole pixels.
    pub fn with_clip(self, clip: Option<Rect>) -> Result<Self, SceneError> {
        Self::new(
            self.shape,
            self.transform,
            self.extent,
            clip,
            self.opacity,
            self.blur,
        )
    }

    /// The backdrop blur region in local coordinates.
    pub const fn shape(self) -> RoundedRect {
        self.shape
    }
    /// Maps [`Self::shape`] into the target.
    pub const fn transform(self) -> Affine {
        self.transform
    }
    /// Target-space bounds of the content.
    pub const fn extent(self) -> Rect {
        self.extent
    }
    /// Target-space rectangle limiting the composite and the blur.
    pub const fn clip(self) -> Option<Rect> {
        self.clip
    }
    /// Opacity of the composited content.
    pub const fn opacity(self) -> f32 {
        self.opacity
    }
    /// Backdrop blur standard deviation in local units.
    pub const fn backdrop_blur(self) -> f32 {
        self.blur
    }
    /// Backdrop blur standard deviation in target pixels: the local one
    /// scaled by the transform's area scale.
    pub fn target_blur(self) -> f32 {
        let [a, b, c, d, ..] = self.transform.coefficients();
        self.blur * sqrt((a * d - b * c).abs())
    }
    /// The target-space bounding box of the blur region.
    pub fn shape_bounds(self) -> Rect {
        self.transform.bounds(self.shape.rect(), 0.0)
    }
}

/// One box of the three-box Gaussian approximation: output pixel `x` is the
/// mean of the `size` source pixels starting at `x - left`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlurBox {
    /// Pixels before the output pixel.
    pub left: u32,
    /// Pixels averaged.
    pub size: u32,
}

/// The three successive box blurs approximating a Gaussian of standard
/// deviation `sigma` pixels, per axis, as SVG and CSS filters specify them:
/// `d = ⌊σ·3·√(2π)/4 + 0.5⌋`; odd `d` gives three centered boxes of `d`,
/// even `d` two boxes of `d` offset left and right and one centered box of
/// `d + 1`, with `d` at most 4095. `None` when `d ≤ 1`, which leaves pixels
/// unchanged.
pub fn blur_boxes(sigma: f32) -> Option<[BlurBox; 3]> {
    // Truncation is the floor of this nonnegative value.
    let d = (sigma * 1.879_971_7 + 0.5).min(4095.0) as u32;
    if d <= 1 {
        return None;
    }
    let half = d / 2;
    Some(if d % 2 == 1 {
        [BlurBox {
            left: half,
            size: d,
        }; 3]
    } else {
        [
            BlurBox {
                left: half,
                size: d,
            },
            BlurBox {
                left: half - 1,
                size: d,
            },
            BlurBox {
                left: half,
                size: d + 1,
            },
        ]
    })
}

/// How far, in pixels, the three boxes of [`blur_boxes`] read beyond an
/// output pixel on either side.
pub fn blur_reach(boxes: &[BlurBox; 3]) -> u32 {
    boxes.iter().map(|b| b.left.max(b.size - 1 - b.left)).sum()
}

/// Square root without `std`, exact to `f32` rounding for the finite
/// nonnegative values used here.
fn sqrt(value: f32) -> f32 {
    if value <= 0.0 {
        return 0.0;
    }
    // Newton's method from the exponent-halved bit pattern.
    let mut x = f32::from_bits((value.to_bits() >> 1) + 0x1fbd_1df5);
    for _ in 0..4 {
        x = 0.5 * (x + value / x);
    }
    x
}
