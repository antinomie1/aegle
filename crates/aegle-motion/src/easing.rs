//! Progress curves: the four quadratic presets, custom cubic Bézier curves
//! and damped springs.

use crate::InvalidValue;
use std::time::Duration;

/// A finite interpolation curve over normalized elapsed time.
///
/// Bézier curves whose control points leave `0..=1` vertically and
/// underdamped springs overshoot, so their progress briefly leaves `0..=1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
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
    /// A CSS-style cubic Bézier curve from `(0, 0)` to `(1, 1)`, see
    /// [`Easing::cubic_bezier`].
    CubicBezier(CubicBezier),
    /// A damped spring stretched over the duration, see [`Spring`].
    Spring(Spring),
}

impl Easing {
    /// A cubic Bézier curve with control points `(x1, y1)` and `(x2, y2)`,
    /// as CSS `cubic-bezier()`: `x1` and `x2` must lie in `0..=1`, the `y`
    /// values may overshoot. Fails for nonfinite values.
    pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> Result<Self, InvalidValue> {
        let x = 0.0..=1.0;
        if !(x.contains(&x1) && x.contains(&x2) && y1.is_finite() && y2.is_finite()) {
            return Err(InvalidValue);
        }
        Ok(Self::CubicBezier(CubicBezier { x1, y1, x2, y2 }))
    }

    /// Progress at normalized time `t` in `0..=1`.
    pub fn sample(self, t: f32) -> f32 {
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => t * (2.0 - t),
            Self::EaseInOut if t < 0.5 => 2.0 * t * t,
            Self::EaseInOut => 1.0 - 2.0 * (1.0 - t) * (1.0 - t),
            Self::CubicBezier(curve) => curve.sample(t),
            Self::Spring(spring) => spring.position(t * spring.settle),
        }
    }
}

/// Control points of a validated [`Easing::CubicBezier`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl CubicBezier {
    /// The curve's `y` where its `x` is `t`.
    fn sample(self, t: f32) -> f32 {
        // Polynomial coefficients of each axis, a·s³ + b·s² + c·s.
        let axis = |p1: f32, p2: f32| {
            let c = 3.0 * p1;
            let b = 3.0 * (p2 - p1) - c;
            (1.0 - c - b, b, c)
        };
        let (ax, bx, cx) = axis(self.x1, self.x2);
        let (ay, by, cy) = axis(self.y1, self.y2);
        let x = |s: f32| ((ax * s + bx) * s + cx) * s;
        // Newton's method converges in a few steps for typical curves;
        // bisection covers flat derivatives. x is monotonic since x1, x2 ∈ 0..=1.
        let mut s = t;
        for _ in 0..8 {
            let error = x(s) - t;
            if error.abs() < 1e-6 {
                return ((ay * s + by) * s + cy) * s;
            }
            let slope = (3.0 * ax * s + 2.0 * bx) * s + cx;
            if slope.abs() < 1e-6 {
                break;
            }
            s -= error / slope;
        }
        let (mut low, mut high) = (0.0, 1.0);
        s = t;
        for _ in 0..32 {
            if x(s) < t {
                low = s;
            } else {
                high = s;
            }
            s = (low + high) / 2.0;
        }
        ((ay * s + by) * s + cy) * s
    }
}

/// A damped spring of unit mass moving from 0 to 1 from rest.
///
/// `stiffness` sets its speed and `damping` its friction: damping below
/// `2·√stiffness` bounces, at it settles fastest without overshoot, above it
/// creeps. [`Spring::duration`] is the time until it stays within 0.1 % of
/// its target, which [`crate::Transition::spring`] uses as the duration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    omega: f32,
    zeta: f32,
    /// Settling time in seconds.
    settle: f32,
}

impl Spring {
    /// A spring with positive finite `stiffness` and `damping`, for example
    /// `Spring::new(170.0, 26.0)` for a quick settle or `(300.0, 10.0)` for a
    /// bounce.
    pub fn new(stiffness: f32, damping: f32) -> Result<Self, InvalidValue> {
        if !(stiffness.is_finite() && damping.is_finite() && stiffness > 0.0 && damping > 0.0) {
            return Err(InvalidValue);
        }
        let omega = stiffness.sqrt();
        let zeta = damping / (2.0 * omega);
        let thousand = 1000f32.ln();
        let settle = if zeta < 0.999 {
            thousand / (zeta * omega)
        } else if zeta <= 1.001 {
            // e^(-ωt)·(1 + ωt) < 0.001 at ωt ≈ 9.23.
            9.23 / omega
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let (slow, fast) = (omega * (zeta - root), omega * (zeta + root));
            (1000.0 * fast / (fast - slow)).ln() / slow
        };
        if !(settle.is_finite() && settle > 0.0) {
            return Err(InvalidValue);
        }
        Ok(Self {
            omega,
            zeta,
            settle,
        })
    }

    /// Time until the spring stays within 0.1 % of its target.
    pub fn duration(self) -> Duration {
        Duration::from_secs_f32(self.settle)
    }

    /// Displacement toward the target `seconds` after release.
    fn position(self, seconds: f32) -> f32 {
        let (omega, zeta, t) = (self.omega, self.zeta, seconds);
        if zeta < 0.999 {
            let damped = omega * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega * t).exp();
            1.0 - decay * ((damped * t).cos() + zeta * omega / damped * (damped * t).sin())
        } else if zeta <= 1.001 {
            1.0 - (-omega * t).exp() * (1.0 + omega * t)
        } else {
            let root = (zeta * zeta - 1.0).sqrt();
            let (r1, r2) = (-omega * (zeta - root), -omega * (zeta + root));
            1.0 + (r2 * (r1 * t).exp() - r1 * (r2 * t).exp()) / (r1 - r2)
        }
    }
}
