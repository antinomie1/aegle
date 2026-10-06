//! Gradients and soft shadows rendered once into shared scene images.
//!
//! Draw the returned [`Image`] stretched over a rect, for example as a widget
//! image. To paint a shape directly, prefer the scene's resolution-independent
//! `SceneBuilder::fill_gradient` and `SceneBuilder::shadow`.
//! Colors interpolate in premultiplied linear light, like motion does; the
//! pixels are straight sRGB RGBA8. Images are at most 64 MiB.

use aegle_scene::{Image, SceneError};
use aegle_types::{Color, color_math};

/// Largest generated image, in bytes.
const MAX_BYTES: usize = 64 << 20;

/// A color at an offset along a gradient, shared with scene gradients.
pub use aegle_scene::GradientStop as Stop;

/// Fills a `width` × `height` image with a linear gradient. The direction is
/// `angle` radians clockwise from left-to-right, so `FRAC_PI_2` runs top to
/// bottom; the first and last stops touch the corners the line passes through.
/// Stops are 2–16 with nondecreasing offsets in `0..=1`.
pub fn linear_gradient(
    width: u32,
    height: u32,
    angle: f32,
    stops: &[Stop],
) -> Result<Image, SceneError> {
    if !angle.is_finite() {
        return Err(SceneError::NonFinite);
    }
    let ramp = Ramp::new(stops)?;
    let (sin, cos) = angle.sin_cos();
    let (w, h) = (width as f32, height as f32);
    let length = (w * cos).abs() + (h * sin).abs();
    fill(width, height, |x, y| {
        let (dx, dy) = (x + 0.5 - w / 2.0, y + 0.5 - h / 2.0);
        ramp.at(0.5 + (dx * cos + dy * sin) / length)
    })
}

/// Fills an image with an elliptical gradient from the center (first stop) to
/// the nearest sides (last stop); corners keep the last color.
pub fn radial_gradient(width: u32, height: u32, stops: &[Stop]) -> Result<Image, SceneError> {
    let ramp = Ramp::new(stops)?;
    let (w, h) = (width as f32 / 2.0, height as f32 / 2.0);
    fill(width, height, |x, y| {
        let (dx, dy) = ((x + 0.5 - w) / w, (y + 0.5 - h) / h);
        ramp.at(dx.hypot(dy))
    })
}

/// A soft shadow for a `width` × `height` rectangle with corner `radius`.
///
/// `blur` is the Gaussian-like softness in pixels. Returns the image and its
/// margin: draw it at the shape's origin minus the margin, over a rectangle
/// larger by twice the margin in each direction.
pub fn shadow(
    width: f32,
    height: f32,
    radius: f32,
    blur: f32,
    color: Color,
) -> Result<(Image, u32), SceneError> {
    if ![width, height, radius, blur]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err(SceneError::NonFinite);
    }
    if width <= 0.0 || height <= 0.0 || radius < 0.0 || blur <= 0.0 {
        return Err(SceneError::NegativeExtent);
    }
    let sigma = blur / 2.0;
    let margin = (sigma * 3.0).ceil();
    let (iw, ih) = (
        (width + 2.0 * margin).ceil(),
        (height + 2.0 * margin).ceil(),
    );
    if iw > Image::MAX_EXTENT as f32 || ih > Image::MAX_EXTENT as f32 {
        return Err(SceneError::InvalidImage);
    }
    let [r, g, b, a] = color.to_rgba();
    let half = (width / 2.0, height / 2.0);
    let radius = radius.min(half.0).min(half.1);
    let image = fill(iw as u32, ih as u32, |x, y| {
        // Signed distance to the rounded rectangle; a logistic curve stands in
        // for the Gaussian edge profile (within 1% of it).
        let p = (x + 0.5 - iw / 2.0, y + 0.5 - ih / 2.0);
        let q = (p.0.abs() - half.0 + radius, p.1.abs() - half.1 + radius);
        let distance = q.0.max(0.0).hypot(q.1.max(0.0)) + q.0.max(q.1).min(0.0) - radius;
        let coverage = 1.0 / (1.0 + (1.702 * distance / sigma).exp());
        [r, g, b, (f32::from(a) * coverage).round() as u8]
    })?;
    Ok((image, margin as u32))
}

fn fill(
    width: u32,
    height: u32,
    mut pixel: impl FnMut(f32, f32) -> [u8; 4],
) -> Result<Image, SceneError> {
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= MAX_BYTES)
        .ok_or(SceneError::InvalidImage)?;
    let mut pixels = Vec::with_capacity(bytes);
    for y in 0..height {
        for x in 0..width {
            pixels.extend(pixel(x as f32, y as f32));
        }
    }
    Image::new(width, height, pixels)
}

/// Validated stops in premultiplied linear light.
struct Ramp {
    stops: Vec<(f32, [f32; 4])>,
}

impl Ramp {
    fn new(stops: &[Stop]) -> Result<Self, SceneError> {
        let ordered = stops
            .windows(2)
            .all(|pair| pair[0].offset <= pair[1].offset);
        if !(2..=aegle_scene::Gradient::MAX_STOPS).contains(&stops.len())
            || !ordered
            || !stops.iter().all(|s| (0.0..=1.0).contains(&s.offset))
        {
            return Err(SceneError::InvalidImage);
        }
        Ok(Self {
            stops: stops
                .iter()
                .map(|s| (s.offset, color_math::linear_rgba(s.color.to_rgba())))
                .collect(),
        })
    }

    /// The color at `t`, clamped to the first and last stops.
    fn at(&self, t: f32) -> [u8; 4] {
        let t = t.clamp(0.0, 1.0);
        let next = self.stops.partition_point(|(offset, _)| *offset <= t);
        let linear = match (next.checked_sub(1), self.stops.get(next)) {
            (None, _) => self.stops[0].1,
            (Some(last), None) => self.stops[last].1,
            (Some(previous), Some((end, to))) => {
                let (start, from) = self.stops[previous];
                let f = (t - start) / (end - start);
                core::array::from_fn(|i| from[i] + (to[i] - from[i]) * f)
            }
        };
        color_math::encoded_rgba(linear)
    }
}
