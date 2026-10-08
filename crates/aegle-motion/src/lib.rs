//! Tween and keyframe sampling, independent of a window, timer or runtime.
//!
//! The caller supplies elapsed time. [`Tween`] interpolates between two values
//! with an [`Easing`]: a quadratic preset, a cubic Bézier curve or a damped
//! [`Spring`]. [`Animation`] adds keyframes, a start delay and repeated cycles. To
//! retarget, construct a new tween from the old one's current sample and restart
//! the caller's elapsed time. A host stores only active animations and stops
//! scheduling frames once they finish. Only multi-keyframe animations allocate.
//! Color interpolation uses premultiplied linear light and the same shared sRGB
//! transfer tables as software rendering. This crate requires `std`.

mod animation;
mod easing;

pub use animation::{Animation, Cycles, Keyframe};
pub use easing::{CubicBezier, Easing, Spring};

use aegle_types::{Color, Point, Shadow, color_math};
use std::fmt;
pub use std::time::Duration;

/// Timing shared by a host's automatic property transitions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    /// Time to reach a new target; zero snaps directly to the target.
    pub duration: Duration,
    /// Progress curve over the duration.
    pub easing: Easing,
}

impl Transition {
    /// Creates transition timing without creating a clock or scheduling work.
    pub const fn new(duration: Duration, easing: Easing) -> Self {
        Self { duration, easing }
    }
    /// Timing that follows `spring` for its natural settling time.
    pub fn spring(spring: Spring) -> Self {
        Self::new(spring.duration(), Easing::Spring(spring))
    }
}

impl Default for Transition {
    fn default() -> Self {
        Self::new(Duration::from_millis(120), Easing::EaseOut)
    }
}

/// A value that can be validated and interpolated by a [`Tween`].
///
/// Implementations must return finite, valid values for every finite
/// progress, which overshooting curves take slightly outside `0..=1`; values
/// with a limited range clamp it. Sampling is a pure computation: it must
/// not mutate controls or dispatch callbacks while a host advances its state.
pub trait Interpolate: Copy {
    /// Validates an endpoint before the tween accepts it.
    fn validate(self) -> Result<(), InvalidValue>;
    /// Interpolates validated endpoints at finite progress.
    fn interpolate(self, to: Self, progress: f32) -> Self;
}

impl Interpolate for f32 {
    fn validate(self) -> Result<(), InvalidValue> {
        if self.is_finite() {
            Ok(())
        } else {
            Err(InvalidValue)
        }
    }

    fn interpolate(self, to: Self, progress: f32) -> Self {
        // The wider intermediate avoids overflowing a finite endpoint interval.
        (self as f64 * (1.0 - progress as f64) + to as f64 * progress as f64) as f32
    }
}

impl Interpolate for Point {
    fn validate(self) -> Result<(), InvalidValue> {
        self.x.validate()?;
        self.y.validate()
    }

    fn interpolate(self, to: Self, progress: f32) -> Self {
        Self::new(
            self.x.interpolate(to.x, progress),
            self.y.interpolate(to.y, progress),
        )
    }
}

impl Interpolate for Shadow {
    fn validate(self) -> Result<(), InvalidValue> {
        self.is_valid().then_some(()).ok_or(InvalidValue)
    }

    fn interpolate(self, to: Self, progress: f32) -> Self {
        Self {
            offset: self.offset.interpolate(to.offset, progress),
            // Overshooting curves must not make the blur negative.
            blur: self.blur.interpolate(to.blur, progress).max(0.0),
            spread: self.spread.interpolate(to.spread, progress),
            color: self.color.interpolate(to.color, progress),
        }
    }
}

impl Interpolate for Color {
    fn validate(self) -> Result<(), InvalidValue> {
        Ok(())
    }

    fn interpolate(self, to: Self, progress: f32) -> Self {
        if progress <= 0.0 || self == to {
            return self;
        }
        if progress >= 1.0 {
            return to;
        }
        let from = color_math::linear_rgba(self.to_rgba());
        let to = color_math::linear_rgba(to.to_rgba());
        let result = color_math::encoded_rgba(std::array::from_fn(|i| {
            from[i].interpolate(to[i], progress)
        }));
        Self::rgba(result[0], result[1], result[2], result[3])
    }
}

/// A validated transition with no ownership of clocks, callbacks or heap memory.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween<T> {
    from: T,
    to: T,
    duration: Duration,
    easing: Easing,
}

impl<T: Interpolate> Tween<T> {
    /// Creates a tween after validating both endpoints.
    ///
    /// A zero duration completes immediately, including when elapsed is zero.
    pub fn new(from: T, to: T, duration: Duration, easing: Easing) -> Result<Self, InvalidValue> {
        from.validate()?;
        to.validate()?;
        Ok(Self {
            from,
            to,
            duration,
            easing,
        })
    }

    /// Samples from caller-supplied elapsed time, clamping to exact endpoints.
    ///
    /// Calls may arrive in any order. For a continuous running animation, the
    /// host supplies elapsed time from its own monotonic clock.
    pub fn sample(&self, elapsed: Duration) -> T {
        if self.finished(elapsed) {
            return self.to;
        }
        if elapsed.is_zero() {
            return self.from;
        }
        let progress = (elapsed.as_secs_f64() / self.duration.as_secs_f64()) as f32;
        self.from.interpolate(self.to, self.easing.sample(progress))
    }

    /// The logical destination, allowing a host to detect a changed target.
    pub fn target(&self) -> T {
        self.to
    }

    /// Whether this duration has elapsed, so a host can remove the active tween.
    pub fn finished(&self, elapsed: Duration) -> bool {
        elapsed >= self.duration
    }
}

/// A nonfinite scalar or point endpoint was supplied to a tween.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidValue;

impl fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("tween endpoints must be finite")
    }
}

impl std::error::Error for InvalidValue {}
