use core::fmt;

/// A finite numeric interval and its clamped, optionally stepped value.
///
/// The step grid starts at `min`; `max` is always an additional valid endpoint.
/// Quantization chooses the nearest candidate, breaking ties toward the larger
/// value. Steps at or below twice machine epsilon times the largest magnitude
/// of `min`, value and their distance leave the value unchanged. This accounts
/// for grid arithmetic rounding and avoids overflowing an unresolvable index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Range {
    min: f64,
    max: f64,
    value: f64,
    step: f64,
}

impl Range {
    /// Creates a range with finite `min < max`, finite span, finite value, and
    /// finite nonnegative step. Zero step selects continuous values.
    pub fn new(min: f64, max: f64, value: f64, step: f64) -> Result<Self, RangeError> {
        validate_bounds(min, max)?;
        validate_value(value)?;
        validate_step(step)?;
        let mut range = Self {
            min,
            max,
            value,
            step,
        };
        range.value = range.quantize(value);
        Ok(range)
    }
    /// Lower endpoint.
    pub fn min(&self) -> f64 {
        self.min
    }
    /// Upper endpoint, also reachable when it is outside the step grid.
    pub fn max(&self) -> f64 {
        self.max
    }
    /// Current clamped and quantized value.
    pub fn value(&self) -> f64 {
        self.value
    }
    /// Step distance; zero means continuous values.
    pub fn step(&self) -> f64 {
        self.step
    }
    /// Current value normalized into `0..=1`.
    pub fn fraction(&self) -> f64 {
        (self.value - self.min) / (self.max - self.min)
    }

    /// Sets a finite value, clamping and quantizing it. Returns whether it changed.
    /// Invalid input leaves the range unchanged.
    pub fn set_value(&mut self, value: f64) -> Result<bool, RangeError> {
        validate_value(value)?;
        if value == self.value {
            return Ok(false);
        }
        let value = self.quantize(value);
        let changed = self.value != value;
        self.value = value;
        Ok(changed)
    }
    /// Changes both endpoints and requantizes the existing value. Returns whether
    /// either bounds or value changed; invalid bounds leave the range unchanged.
    pub fn set_bounds(&mut self, min: f64, max: f64) -> Result<bool, RangeError> {
        validate_bounds(min, max)?;
        if min == self.min && max == self.max {
            return Ok(false);
        }
        let mut next = Self { min, max, ..*self };
        next.value = next.quantize(self.value);
        let changed = *self != next;
        *self = next;
        Ok(changed)
    }
    /// Changes the step and requantizes the existing value. Returns whether step
    /// or value changed; invalid steps leave the range unchanged.
    pub fn set_step(&mut self, step: f64) -> Result<bool, RangeError> {
        validate_step(step)?;
        if step == self.step {
            return Ok(false);
        }
        let mut next = Self { step, ..*self };
        next.value = next.quantize(self.value);
        let changed = *self != next;
        *self = next;
        Ok(changed)
    }

    fn quantize(&self, value: f64) -> f64 {
        let value = value.clamp(self.min, self.max);
        if self.step == 0.0 || value == self.min || value == self.max {
            return value;
        }
        let distance = value - self.min;
        let resolution = self.min.abs().max(value.abs()).max(distance) * (2.0 * f64::EPSILON);
        if self.step <= resolution {
            return value;
        }
        let grid =
            (self.min + (distance / self.step).round() * self.step).clamp(self.min, self.max);
        if self.max - value <= (grid - value).abs() {
            self.max
        } else {
            grid
        }
    }
}

fn validate_bounds(min: f64, max: f64) -> Result<(), RangeError> {
    if !min.is_finite() || !max.is_finite() || min >= max || !(max - min).is_finite() {
        return Err(RangeError::InvalidBounds);
    }
    Ok(())
}
fn validate_value(value: f64) -> Result<(), RangeError> {
    if !value.is_finite() {
        return Err(RangeError::InvalidValue);
    }
    Ok(())
}
fn validate_step(step: f64) -> Result<(), RangeError> {
    if !step.is_finite() || step < 0.0 {
        return Err(RangeError::InvalidStep);
    }
    Ok(())
}

/// Invalid input at a numeric range or slider boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RangeError {
    /// Endpoints are nonfinite, unordered, or their span overflows.
    InvalidBounds,
    /// The requested value is not finite.
    InvalidValue,
    /// Step is negative or nonfinite.
    InvalidStep,
    /// Slider extent is negative/nonfinite or pointer coordinates are nonfinite.
    InvalidExtent,
}
impl fmt::Display for RangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidBounds => "range requires finite ordered bounds and finite span",
            Self::InvalidValue => "range value must be finite",
            Self::InvalidStep => "range step must be finite and nonnegative",
            Self::InvalidExtent => "slider requires finite coordinates and nonnegative extent",
        })
    }
}
impl core::error::Error for RangeError {}
