//! Maps platform input timestamps onto [`Instant`].

use std::time::{Duration, Instant};

/// Platform event times are milliseconds with an undefined base (Wayland) or
/// since boot (Win32), wrapping at `u32::MAX`. Each event is placed relative
/// to the previous one; an event that would land after its delivery moves the
/// anchor back to the delivery time, so the mapping converges on the
/// smallest observed delivery latency and never runs ahead of the clock.
#[derive(Default)]
pub(crate) struct EventClock {
    anchor: Option<(Instant, u32)>,
}

impl EventClock {
    pub fn at(&mut self, time: u32) -> Instant {
        let now = Instant::now();
        let at = match self.anchor {
            Some((anchor, base)) => {
                let delta = time.wrapping_sub(base) as i32;
                let offset = Duration::from_millis(u64::from(delta.unsigned_abs()));
                let at = if delta >= 0 {
                    anchor.checked_add(offset)
                } else {
                    anchor.checked_sub(offset)
                };
                at.map_or(now, |at| at.min(now))
            }
            None => now,
        };
        self.anchor = Some((at, time));
        at
    }
}
