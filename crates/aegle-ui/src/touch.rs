use crate::{Modifiers, Point, PointerId, PointerKind, Result, TouchPhase, Ui, UiError};

/// Logical pixels a finger travels before a tap on scrollable content becomes a pan.
const SLOP: f32 = 10.0;
/// Samples older than this, in milliseconds, do not count toward fling velocity.
const WINDOW: u32 = 100;

/// One finger in contact.
pub struct Finger {
    id: PointerId,
    start: Point,
    last: Point,
    /// The finger went down on a control that uses drags (slider, editor, scroll bar).
    drags: bool,
    panning: bool,
    samples: Vec<(u32, Point)>,
}

impl Ui {
    /// Delivers one finger event in logical window coordinates. `id` must differ
    /// from every other pointer in this window; `time` is a millisecond clock.
    ///
    /// Taps and drags reach controls like primary-button pointer events, so
    /// buttons, sliders and text selection work unchanged. A finger that goes down
    /// on anything else and moves beyond 10 px cancels the tap and pans the
    /// scrollable content under it, nested views included; lifting it while it is
    /// still moving continues with momentum (with the `motion` feature). Hover
    /// state does not outlive the contact.
    pub fn touch(&self, id: PointerId, phase: TouchPhase, position: Point, time: u32) -> Result {
        if !position.x.is_finite() || !position.y.is_finite() {
            return Err(UiError::InvalidValue.into());
        }
        let mods = Modifiers::default();
        match phase {
            TouchPhase::Down => {
                #[cfg(feature = "motion")]
                self.stop_fling();
                let drags = self.drags_at(position);
                let mut state = self.write();
                state.fingers.retain(|finger| finger.id != id);
                state.fingers.push(Finger {
                    id,
                    start: position,
                    last: position,
                    drags,
                    panning: false,
                    samples: vec![(time, position)],
                });
                drop(state);
                self.pointer(id, PointerKind::Down { clicks: 1 }, position, mods)
            }
            TouchPhase::Move => {
                let mut state = self.write();
                let Some(finger) = state.fingers.iter_mut().find(|f| f.id == id) else {
                    return Ok(());
                };
                finger
                    .samples
                    .retain(|(at, _)| time.wrapping_sub(*at) <= WINDOW);
                finger.samples.push((time, position));
                let (dx, dy) = (position.x - finger.start.x, position.y - finger.start.y);
                let started = !finger.panning && !finger.drags && dx.hypot(dy) > SLOP;
                finger.panning |= started;
                let previous = std::mem::replace(&mut finger.last, position);
                let panning = finger.panning;
                drop(state);
                if started {
                    // The tap is over: release the control without activating it.
                    self.pointer(id, PointerKind::Cancel, position, mods)?;
                }
                if panning {
                    let delta = Point::new(previous.x - position.x, previous.y - position.y);
                    self.scroll_by(position, delta)
                } else {
                    self.pointer(id, PointerKind::Move, position, mods)
                }
            }
            TouchPhase::Up | TouchPhase::Cancel => {
                let finger = {
                    let mut state = self.write();
                    let Some(index) = state.fingers.iter().position(|f| f.id == id) else {
                        return Ok(());
                    };
                    state.fingers.swap_remove(index)
                };
                if finger.panning {
                    #[cfg(feature = "motion")]
                    if phase == TouchPhase::Up {
                        self.release_pan(&finger, position, time);
                    }
                    return Ok(());
                }
                let kind = if phase == TouchPhase::Up {
                    PointerKind::Up
                } else {
                    PointerKind::Cancel
                };
                self.pointer(id, kind, position, mods)?;
                // A finger has no hover: clear it after the contact.
                self.pointer_leave()
            }
        }
    }

    /// Whether the control under `position` handles drags itself.
    fn drags_at(&self, position: Point) -> bool {
        let mut state = self.write();
        state.rebuild_order();
        let control = state
            .hit(position)
            .is_some_and(|id| state.tree.get(id).unwrap().context.control.drags());
        control || state.scrollbar_at(position, None).is_some()
    }

    #[cfg(feature = "motion")]
    fn release_pan(&self, finger: &Finger, position: Point, time: u32) {
        let recent: Vec<_> = finger
            .samples
            .iter()
            .filter(|(at, _)| time.wrapping_sub(*at) <= WINDOW)
            .collect();
        let (Some(first), Some(last)) = (recent.first(), recent.last()) else {
            return;
        };
        let span = last.0.wrapping_sub(first.0) as f32 / 1000.0;
        if recent.len() < 2 || span <= 0.0 {
            return;
        }
        let velocity = Point::new((first.1.x - last.1.x) / span, (first.1.y - last.1.y) / span);
        self.fling(position, velocity)
    }
}
