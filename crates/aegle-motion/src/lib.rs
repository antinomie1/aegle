//! Allocation-free tween sampling, independent of a window, timer or runtime.
//!
//! The caller supplies elapsed time. To retarget, construct a new [`Tween`] from
//! the old tween's current sample and restart the caller's elapsed time. A host
//! stores only active tweens and stops scheduling frames once they finish.
//! Color interpolation uses premultiplied linear light and the same shared sRGB
//! transfer tables as software rendering. This crate requires `std`.

use aegle_types::{Color, Point, color_math};
use std::fmt;
pub use std::time::Duration;

/// A finite interpolation curve over normalized elapsed time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    /// Constant interpolation speed.
    #[default]
    Linear,
    /// Quadratic acceleration from rest.
    EaseIn,
    /// Quadratic deceleration to rest.
    EaseOut,
    /// Symmetric quadratic acceleration and deceleration.
    EaseInOut,
}

impl Easing {
    fn sample(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => t * (2.0 - t),
            Self::EaseInOut if t < 0.5 => 2.0 * t * t,
            Self::EaseInOut => 1.0 - 2.0 * (1.0 - t) * (1.0 - t),
        }
    }
}

/// Timing shared by a host's automatic property transitions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
}

impl Default for Transition {
    fn default() -> Self {
        Self::new(Duration::from_millis(120), Easing::EaseOut)
    }
}

/// A value that can be validated and interpolated by a [`Tween`].
///
/// Implementations must return finite, valid values between validated endpoints
/// for every finite progress in `0..=1`. Sampling is a pure computation: it must
/// not mutate controls or dispatch callbacks while a host advances its state.
pub trait Interpolate: Copy {
    /// Validates an endpoint before the tween accepts it.
    fn validate(self) -> Result<(), InvalidValue>;
    /// Interpolates validated endpoints at finite progress in `0..=1`.
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

impl Interpolate for Color {
    fn validate(self) -> Result<(), InvalidValue> {
        Ok(())
    }

    fn interpolate(self, to: Self, progress: f32) -> Self {
        if progress == 0.0 || self == to {
            return self;
        }
        if progress == 1.0 {
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
