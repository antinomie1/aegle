use crate::{
    Appearance, Point, Result, Transform, Transition, UiError, callbacks::Handler, state::State,
};
use aegle_core::{Dirty, NodeId};
use aegle_motion::{Interpolate, InvalidValue, Tween};
use std::{collections::HashMap, time::Duration};

#[derive(Default)]
pub(crate) struct Motion {
    pub now: Duration,
    pub reduced: bool,
    pub default: Option<Transition>,
    pub tracks: HashMap<NodeId, Track>,
    pub active: HashMap<NodeId, Active>,
    /// Running offset transitions; the presented value lives on the element.
    pub moving: HashMap<NodeId, Moving>,
    /// Running scale/rotation transitions.
    pub turning: HashMap<NodeId, Turning>,
    /// Momentum scrolling after a finished touchpad gesture.
    pub fling: Option<crate::fling::Fling>,
    /// Completion handlers, versioned in the shared callback sequence.
    pub ends: HashMap<NodeId, Handler>,
}

pub(crate) struct Track {
    pub timing: Transition,
    pub presented: Option<Appearance>,
}

pub(crate) struct Active {
    tween: Tween<Paint>,
    start: Duration,
}

pub(crate) struct Moving {
    pub tween: Tween<Point>,
    /// Set by the next refresh, as for paint, so a request made from a callback
    /// starts at the host's current time rather than the last sampled one.
    start: Option<Duration>,
}

pub(crate) struct Turning {
    pub tween: Tween<Spin>,
    start: Option<Duration>,
}

#[derive(Clone, Copy)]
pub(crate) struct Spin(pub Transform);

impl Interpolate for Spin {
    fn validate(self) -> std::result::Result<(), InvalidValue> {
        if self.0.valid() {
            Ok(())
        } else {
            Err(InvalidValue)
        }
    }
    fn interpolate(self, to: Self, progress: f32) -> Self {
        Self(Transform {
            scale: self.0.scale.interpolate(to.0.scale, progress),
            rotation: self.0.rotation.interpolate(to.0.rotation, progress),
        })
    }
}

#[derive(Clone, Copy)]
struct Paint(Appearance);

impl Interpolate for Paint {
    fn validate(self) -> std::result::Result<(), InvalidValue> {
        self.0.validate().map_err(|_| InvalidValue)
    }
    fn interpolate(self, to: Self, progress: f32) -> Self {
        let a = self.0;
        let b = to.0;
        Self(Appearance {
            background: a.background.interpolate(b.background, progress),
            foreground: a.foreground.interpolate(b.foreground, progress),
            border_color: a.border_color.interpolate(b.border_color, progress),
            border_width: a.border_width.interpolate(b.border_width, progress),
            radius: a.radius.interpolate(b.radius, progress),
            focus_color: a.focus_color.interpolate(b.focus_color, progress),
            focus_width: a.focus_width.interpolate(b.focus_width, progress),
            selection: a.selection.interpolate(b.selection, progress),
            caret: a.caret.interpolate(b.caret, progress),
            indicator: a.indicator.interpolate(b.indicator, progress),
        })
    }
}

impl State {
    /// Called only for invalidated records, after the new skin target is validated.
    pub fn transition_appearance(&mut self, id: NodeId, target: Appearance) -> Result<Appearance> {
        let Some(track) = self.motion.tracks.get_mut(&id) else {
            return Ok(target);
        };
        let Some(current) = track.presented else {
            track.presented = Some(target);
            return Ok(target);
        };
        if self.motion.reduced
            || track.timing.duration.is_zero()
            || !self.tree.get(id).unwrap().context.effective_visible
        {
            // A change that snaps instead of animating still completes.
            track.presented = Some(target);
            if self.motion.active.remove(&id).is_some() || current != target {
                self.complete(id);
            }
            return Ok(target);
        }
        if self
            .motion
            .active
            .get(&id)
            .is_some_and(|active| active.tween.target().0 == target)
        {
            return Ok(current);
        }
        if current == target {
            if self.motion.active.remove(&id).is_some() {
                self.complete(id);
            }
        } else {
            self.motion.active.insert(
                id,
                Active {
                    tween: Tween::new(
                        Paint(current),
                        Paint(target),
                        track.timing.duration,
                        track.timing.easing,
                    )?,
                    start: self.motion.now,
                },
            );
        }
        Ok(current)
    }

    pub fn presented_appearance(&self, id: NodeId) -> Result<Appearance> {
        self.motion
            .tracks
            .get(&id)
            .and_then(|track| track.presented)
            .map(Ok)
            .unwrap_or_else(|| self.appearance(id))
    }

    /// Starts, retargets or snaps an offset. Without a running policy the
    /// offset snaps; a policy's snap (reduced motion, hidden, zero duration)
    /// still completes.
    pub fn transition_offset(&mut self, id: NodeId, target: Point) -> Result {
        let element = &self.tree.get(id).unwrap().context;
        let current = element.offset;
        let timing = self.motion.tracks.get(&id).map(|track| track.timing);
        let running = self.motion.moving.get(&id).map(|a| a.tween.target());
        if running == Some(target) || (running.is_none() && current == target) {
            return Ok(());
        }
        if let Some(timing) = timing.filter(|timing| {
            !self.motion.reduced && !timing.duration.is_zero() && element.effective_visible
        }) {
            let tween = Tween::new(current, target, timing.duration, timing.easing)?;
            self.motion.moving.insert(id, Moving { tween, start: None });
            self.repaint = true;
            return Ok(());
        }
        self.motion.moving.remove(&id);
        self.tree.get_mut(id).unwrap().context.offset = target;
        self.geometry_dirty = true;
        self.repaint = true;
        if timing.is_some() {
            self.complete(id);
        }
        Ok(())
    }

    /// Starts, retargets or snaps a scale/rotation like [`Self::transition_offset`].
    pub fn transition_spin(&mut self, id: NodeId, target: Transform) -> Result {
        let element = &self.tree.get(id).unwrap().context;
        let current = element.spin;
        let timing = self.motion.tracks.get(&id).map(|track| track.timing);
        let running = self.motion.turning.get(&id).map(|a| a.tween.target().0);
        if running == Some(target) || (running.is_none() && current == target) {
            return Ok(());
        }
        if let Some(timing) = timing.filter(|timing| {
            !self.motion.reduced && !timing.duration.is_zero() && element.effective_visible
        }) {
            let tween = Tween::new(Spin(current), Spin(target), timing.duration, timing.easing)?;
            self.motion
                .turning
                .insert(id, Turning { tween, start: None });
            self.repaint = true;
            return Ok(());
        }
        self.motion.turning.remove(&id);
        self.set_spin(id, target);
        if timing.is_some() {
            self.complete(id);
        }
        Ok(())
    }

    /// Jumps a running scale/rotation to its target. Returns whether one was running.
    pub fn snap_spin(&mut self, id: NodeId) -> bool {
        let Some(active) = self.motion.turning.remove(&id) else {
            return false;
        };
        self.set_spin(id, active.tween.target().0);
        true
    }

    /// Starts offset and spin transitions requested since the last refresh.
    pub fn start_offsets(&mut self) {
        for moving in self.motion.moving.values_mut() {
            moving.start.get_or_insert(self.motion.now);
        }
        for turning in self.motion.turning.values_mut() {
            turning.start.get_or_insert(self.motion.now);
        }
        if let Some(fling) = &mut self.motion.fling {
            fling.last.get_or_insert(self.motion.now);
        }
    }

    /// Jumps a running offset to its target. Returns whether one was running.
    pub fn snap_offset(&mut self, id: NodeId) -> bool {
        let Some(active) = self.motion.moving.remove(&id) else {
            return false;
        };
        self.tree.get_mut(id).unwrap().context.offset = active.tween.target();
        self.geometry_dirty = true;
        self.repaint = true;
        true
    }

    /// Queues the completion handler once none of the node's transitions remain.
    pub fn complete(&mut self, id: NodeId) {
        let motion = &self.motion;
        if !motion.active.contains_key(&id)
            && !motion.moving.contains_key(&id)
            && !motion.turning.contains_key(&id)
        {
            if let Some(handler) = motion.ends.get(&id) {
                self.pending.push_back((id, handler.version));
            }
        }
    }

    pub fn advance_animations(&mut self, now: Duration) -> Result {
        if now < self.motion.now {
            return Err(UiError::InvalidValue.into());
        }
        self.motion.now = now;
        self.step_fling(now)?;
        let mut moved = false;
        let Motion {
            moving,
            turning,
            active,
            ends,
            ..
        } = &mut self.motion;
        turning.retain(|id, turn| {
            let Some(start) = turn.start else {
                return true;
            };
            let element = &mut self.tree.get_mut(*id).unwrap().context;
            let elapsed = now - start;
            let finished = turn.tween.finished(elapsed) || !element.effective_visible;
            element.spin = if finished {
                turn.tween.target().0
            } else {
                turn.tween.sample(elapsed).0
            };
            moved = true;
            if finished && !active.contains_key(id) && !moving.contains_key(id) {
                if let Some(handler) = ends.get(id) {
                    self.pending.push_back((*id, handler.version));
                }
            }
            !finished
        });
        moving.retain(|id, tween| {
            let Some(start) = tween.start else {
                return true;
            };
            let element = &mut self.tree.get_mut(*id).unwrap().context;
            let elapsed = now - start;
            // Hidden subtrees go straight to their target.
            let finished = tween.tween.finished(elapsed) || !element.effective_visible;
            element.offset = if finished {
                tween.tween.target()
            } else {
                tween.tween.sample(elapsed)
            };
            moved = true;
            if finished && !active.contains_key(id) && !turning.contains_key(id) {
                if let Some(handler) = ends.get(id) {
                    self.pending.push_back((*id, handler.version));
                }
            }
            !finished
        });
        self.geometry_dirty |= moved;
        self.repaint |= moved;
        let tracks = &mut self.motion.tracks;
        let tree = &mut self.tree;
        let (moving, turning, ends, pending) = (
            &self.motion.moving,
            &self.motion.turning,
            &self.motion.ends,
            &mut self.pending,
        );
        self.motion.active.retain(|id, active| {
            let elapsed = now - active.start;
            let next = active.tween.sample(elapsed).0;
            let track = tracks.get_mut(id).unwrap();
            let previous = track.presented.unwrap();
            track.presented = Some(next);
            if next != previous {
                let dirty = if next.foreground != previous.foreground {
                    Dirty::PAINT | Dirty::SEMANTICS
                } else {
                    Dirty::PAINT
                };
                // Removal and window destruction prune both maps before IDs retire.
                tree.mark_dirty(*id, dirty)
                    .expect("animation belongs to a live node");
            }
            let finished = active.tween.finished(elapsed);
            if finished && !moving.contains_key(id) && !turning.contains_key(id) {
                if let Some(handler) = ends.get(id) {
                    pending.push_back((*id, handler.version));
                }
            }
            !finished
        });
        Ok(())
    }
}
