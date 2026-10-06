use crate::{Point, Result, Ui, UiError, state::State};
use std::time::Duration;

/// Time constant of the exponential velocity decay; a fling travels `velocity * TAU`.
const TAU: f32 = 0.325;
/// Speed in logical pixels per second below which momentum ends.
const STOP: f32 = 10.0;

/// Momentum left by a finished touchpad or touch scroll.
pub struct Fling {
    position: Point,
    velocity: Point,
    /// Set by the first refresh after the request, so it starts at host time.
    pub last: Option<Duration>,
}

impl State {
    /// Scrolls by the distance travelled since the previous step.
    pub fn step_fling(&mut self, now: Duration) -> Result {
        let Some(mut fling) = self.motion.fling.take() else {
            return Ok(());
        };
        let Some(last) = fling.last else {
            self.motion.fling = Some(fling);
            return Ok(());
        };
        let dt = now.saturating_sub(last).as_secs_f32();
        if dt == 0.0 {
            self.motion.fling = Some(fling);
            return Ok(());
        }
        let decay = (-dt / TAU).exp();
        let travel = TAU * (1.0 - decay);
        let delta = Point::new(fling.velocity.x * travel, fling.velocity.y * travel);
        fling.velocity = Point::new(fling.velocity.x * decay, fling.velocity.y * decay);
        let moved = self.scroll_by_at(fling.position, delta, None)?;
        if moved && fling.velocity.x.hypot(fling.velocity.y) >= STOP {
            fling.last = Some(now);
            self.motion.fling = Some(fling);
        }
        Ok(())
    }
}

impl Ui {
    /// Continues a finished touchpad or touch scroll with momentum at a window
    /// point. `velocity` is in logical pixels per second, in the same direction
    /// as [`Self::scroll_by`] displacements. The speed decays exponentially and
    /// ends at 10 px/s or when no viewport can move further. A later scroll,
    /// pointer press or [`Self::stop_fling`] cancels it; reduced motion ignores it.
    /// The host drives it through [`Self::advance_animations`].
    pub fn fling(&self, position: Point, velocity: Point) -> Result {
        if ![position.x, position.y, velocity.x, velocity.y]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(UiError::InvalidValue.into());
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.motion.fling = (!state.motion.reduced && velocity.x.hypot(velocity.y) >= STOP)
            .then_some(Fling {
                position,
                velocity,
                last: None,
            });
        Ok(())
    }

    /// Cancels momentum scrolling, for example when a finger touches down.
    pub fn stop_fling(&self) -> Result {
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .motion
            .fling = None;
        Ok(())
    }
}
