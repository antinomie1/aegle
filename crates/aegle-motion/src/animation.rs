//! Explicit keyframe animations with delay, repetition and alternation.

use crate::{Easing, Interpolate, InvalidValue, Transition};
use std::{sync::Arc, time::Duration};

/// A value at a point of an [`Animation`] cycle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe<T> {
    /// Position in the cycle, `0.0` at its start and `1.0` at its end.
    pub at: f32,
    /// The value reached at `at`.
    pub value: T,
    /// The curve of the segment arriving at this keyframe.
    pub easing: Easing,
}

impl<T> Keyframe<T> {
    /// A keyframe reached linearly.
    pub const fn new(at: f32, value: T) -> Self {
        Self {
            at,
            value,
            easing: Easing::Linear,
        }
    }
    /// Reaches this keyframe along `easing` instead.
    pub fn easing(self, easing: Easing) -> Self {
        Self { easing, ..self }
    }
}

/// How many cycles an [`Animation`] plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cycles {
    /// This many cycles in total; zero plays none and ends at the start value.
    Times(u32),
    /// Until stopped. It never completes by itself, so a host keeps
    /// producing frames while it runs.
    #[default]
    Forever,
}

#[derive(Clone, Debug)]
enum Frames<T> {
    /// A two-frame curve, stored without allocating.
    Pair(T, T, Easing),
    Many(Arc<[Keyframe<T>]>),
}

/// A validated, explicitly started animation: keyframes over one cycle, an
/// optional start delay and repeated cycles. Like [`crate::Tween`], it owns no
/// clock; the caller samples it with elapsed time. Cloning shares the frames.
#[derive(Clone, Debug)]
pub struct Animation<T> {
    frames: Frames<T>,
    duration: Duration,
    delay: Duration,
    cycles: Cycles,
    alternate: bool,
}

impl<T: Interpolate> Animation<T> {
    /// Plays `frames` once over `duration`. Keyframes must start at `0.0`,
    /// end at `1.0` and not go backwards, with valid values; a keyframe at
    /// the same position as the previous one jumps to its value.
    pub fn new(
        duration: Duration,
        frames: impl Into<Arc<[Keyframe<T>]>>,
    ) -> Result<Self, InvalidValue> {
        let frames = frames.into();
        let ordered = frames.windows(2).all(|pair| pair[0].at <= pair[1].at);
        if frames.len() < 2 || frames[0].at != 0.0 || frames[frames.len() - 1].at != 1.0 || !ordered
        {
            return Err(InvalidValue);
        }
        frames.iter().try_for_each(|frame| frame.value.validate())?;
        Ok(Self::with(duration, Frames::Many(frames)))
    }

    /// Plays from `from` to `to` once with `timing`, without allocating.
    pub fn tween(from: T, to: T, timing: Transition) -> Result<Self, InvalidValue> {
        from.validate()?;
        to.validate()?;
        let frames = Frames::Pair(from, to, timing.easing);
        Ok(Self::with(timing.duration, frames))
    }

    fn with(duration: Duration, frames: Frames<T>) -> Self {
        Self {
            frames,
            duration,
            delay: Duration::ZERO,
            cycles: Cycles::Times(1),
            alternate: false,
        }
    }

    /// Holds the first value for `delay` before the first cycle.
    pub fn delay(self, delay: Duration) -> Self {
        Self { delay, ..self }
    }
    /// Plays `cycles` cycles instead of one.
    pub fn cycles(self, cycles: Cycles) -> Self {
        Self { cycles, ..self }
    }
    /// Plays every second cycle backwards, so a repeated animation swings
    /// back and forth instead of jumping to its start.
    pub fn alternate(self, alternate: bool) -> Self {
        Self { alternate, ..self }
    }

    /// The value `elapsed` after the animation started.
    pub fn sample(&self, elapsed: Duration) -> T {
        if self.finished(elapsed) || self.duration.is_zero() {
            return self.target();
        }
        let Some(running) = elapsed.checked_sub(self.delay) else {
            return self.at(0.0);
        };
        let cycles = running.as_secs_f64() / self.duration.as_secs_f64();
        let forward = !self.alternate || (cycles as u64).is_multiple_of(2);
        let position = cycles.fract() as f32;
        self.at(if forward { position } else { 1.0 - position })
    }

    /// Whether every cycle has played; never for [`Cycles::Forever`] unless
    /// the cycle has no duration.
    pub fn finished(&self, elapsed: Duration) -> bool {
        if self.duration.is_zero() {
            return elapsed >= self.delay;
        }
        match self.cycles {
            Cycles::Forever => false,
            Cycles::Times(times) => self
                .duration
                .checked_mul(times)
                .and_then(|length| length.checked_add(self.delay))
                .is_none_or(|end| elapsed >= end),
        }
    }

    /// The value the animation rests at once finished: the end of its last
    /// cycle, or of a forward cycle for [`Cycles::Forever`].
    pub fn target(&self) -> T {
        let backwards = match self.cycles {
            Cycles::Times(0) => true,
            Cycles::Times(times) => self.alternate && times.is_multiple_of(2),
            Cycles::Forever => false,
        };
        self.at(if backwards { 0.0 } else { 1.0 })
    }

    /// The value at `position` within one forward cycle.
    fn at(&self, position: f32) -> T {
        match &self.frames {
            Frames::Pair(from, to, easing) => match position {
                0.0 => *from,
                1.0 => *to,
                _ => from.interpolate(*to, easing.sample(position)),
            },
            Frames::Many(frames) => {
                let next = frames
                    .iter()
                    .position(|frame| frame.at >= position)
                    .unwrap()
                    .max(1);
                let (from, to) = (&frames[next - 1], &frames[next]);
                if position >= to.at || to.at == from.at {
                    return to.value;
                }
                let local = (position - from.at) / (to.at - from.at);
                from.value.interpolate(to.value, to.easing.sample(local))
            }
        }
    }
}
