// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.
#![allow(missing_docs)]

use crate::{
    Appearance, Point, Result, Transform, Transition, UiError, callbacks::Handler, state::State,
};
use aegle_core::{Dirty, NodeId};
use aegle_motion::{Interpolate, InvalidValue, Tween};
use std::{collections::HashMap, time::Duration};

/// A presented value whose transition timing can be set on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransitionProperty {
    /// Every paint value of the appearance: colors, border and focus widths,
    /// and radius.
    Paint,
    /// The translation set by `Node::set_offset`.
    Offset,
    /// The scale of `Node::set_transform`.
    Scale,
    /// The rotation of `Node::set_transform`.
    Rotation,
}

impl TransitionProperty {
    /// Every property, in slot order.
    pub const ALL: [Self; 4] = [Self::Paint, Self::Offset, Self::Scale, Self::Rotation];
}

#[derive(Default)]
pub struct Motion {
    pub now: Duration,
    pub reduced: bool,
    pub default: Option<Transition>,
    pub tracks: HashMap<NodeId, Track>,
    pub active: HashMap<NodeId, Active>,
    /// Running geometric transitions; presented values live on the element.
    pub moving: HashMap<NodeId, Running<Point>>,
    pub scaling: HashMap<NodeId, Running<f32>>,
    pub rotating: HashMap<NodeId, Running<f32>>,
    /// Momentum scrolling after a finished touchpad gesture.
    pub fling: Option<crate::fling::Fling>,
    /// Completion handlers, versioned in the shared callback sequence.
    pub ends: HashMap<NodeId, Handler>,
    /// Nodes whose transition finished during the current advance.
    finished: Vec<NodeId>,
}

/// A node's transition policy.
pub struct Track {
    /// Timing per property, in [`TransitionProperty::ALL`] order; `None`
    /// changes that property immediately.
    pub timings: [Option<Transition>; 4],
    pub presented: Option<Appearance>,
}

impl Track {
    /// The same timing for every property.
    pub fn uniform(timing: Transition) -> Self {
        Self {
            timings: [Some(timing); 4],
            presented: None,
        }
    }
    pub fn timing(&self, property: TransitionProperty) -> Option<Transition> {
        self.timings[property as usize]
    }
}

pub struct Active {
    tween: Tween<Paint>,
    start: Duration,
}

pub struct Running<T: Interpolate> {
    pub tween: Tween<T>,
    /// Set by the next refresh, as for paint, so a request made from a callback
    /// starts at the host's current time rather than the last sampled one.
    start: Option<Duration>,
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

/// What a geometric request did.
#[derive(PartialEq)]
enum Step {
    Unchanged,
    Started,
    Snapped,
}

/// Starts or retargets a tween when `timing` animates, otherwise drops any
/// running one so the caller snaps.
fn plan<T: Interpolate + PartialEq>(
    running: &mut HashMap<NodeId, Running<T>>,
    id: NodeId,
    current: T,
    target: T,
    timing: Option<Transition>,
) -> Result<Step> {
    let goal = running.get(&id).map(|r| r.tween.target());
    if goal == Some(target) || (goal.is_none() && current == target) {
        return Ok(Step::Unchanged);
    }
    let Some(timing) = timing else {
        running.remove(&id);
        return Ok(Step::Snapped);
    };
    let tween = Tween::new(current, target, timing.duration, timing.easing)?;
    running.insert(id, Running { tween, start: None });
    Ok(Step::Started)
}

/// Samples running tweens into the tree, recording finished nodes.
fn advance<T: Interpolate>(
    running: &mut HashMap<NodeId, Running<T>>,
    state: &mut aegle_core::Tree<aegle_layout::LayoutNode<crate::state::Element>>,
    now: Duration,
    finished: &mut Vec<NodeId>,
    apply: impl Fn(&mut crate::state::Element, T),
) -> bool {
    let mut moved = false;
    running.retain(|id, run| {
        let Some(start) = run.start else {
            return true;
        };
        let element = &mut state.get_mut(*id).unwrap().context;
        let elapsed = now - start;
        // Hidden subtrees go straight to their target.
        let done = run.tween.finished(elapsed) || !element.effective_visible;
        let value = if done {
            run.tween.target()
        } else {
            run.tween.sample(elapsed)
        };
        apply(element, value);
        moved = true;
        if done {
            finished.push(*id);
        }
        !done
    });
    moved
}

impl Motion {
    /// Drops every transition, policy and handler of a removed node.
    pub fn forget(&mut self, id: NodeId) {
        self.tracks.remove(&id);
        self.active.remove(&id);
        self.moving.remove(&id);
        self.scaling.remove(&id);
        self.rotating.remove(&id);
        self.ends.remove(&id);
    }
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.active.clear();
        self.moving.clear();
        self.scaling.clear();
        self.rotating.clear();
        self.fling = None;
        self.ends.clear();
    }
    /// Whether any transition of `id` runs.
    pub fn running(&self, id: NodeId) -> bool {
        self.active.contains_key(&id)
            || self.moving.contains_key(&id)
            || self.scaling.contains_key(&id)
            || self.rotating.contains_key(&id)
    }
    pub fn any_running(&self) -> bool {
        !(self.active.is_empty()
            && self.moving.is_empty()
            && self.scaling.is_empty()
            && self.rotating.is_empty())
    }
}

impl State {
    /// Whether the policy has a timing for one property, and that timing when
    /// it animates now (motion allowed, nonzero, visible). A control not yet
    /// painted has nothing to animate from, so its geometry is set directly.
    fn timing(&self, id: NodeId, property: TransitionProperty) -> (bool, Option<Transition>) {
        let timing = self.motion.tracks.get(&id).and_then(|t| {
            (property == TransitionProperty::Paint || t.presented.is_some())
                .then(|| t.timing(property))
                .flatten()
        });
        let visible = self.tree.get(id).unwrap().context.effective_visible;
        let animates = timing.filter(|t| !self.motion.reduced && !t.duration.is_zero() && visible);
        (timing.is_some(), animates)
    }

    /// Called only for invalidated records, after the new skin target is validated.
    pub fn transition_appearance(&mut self, id: NodeId, target: Appearance) -> Result<Appearance> {
        let (policy, animates) = self.timing(id, TransitionProperty::Paint);
        let Some(track) = self.motion.tracks.get_mut(&id) else {
            return Ok(target);
        };
        let Some(current) = track.presented else {
            track.presented = Some(target);
            return Ok(target);
        };
        let Some(timing) = animates else {
            // A change that snaps instead of animating still completes.
            track.presented = Some(target);
            if policy && (self.motion.active.remove(&id).is_some() || current != target) {
                self.complete(id);
            }
            return Ok(target);
        };
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
            let tween = Tween::new(
                Paint(current),
                Paint(target),
                timing.duration,
                timing.easing,
            )?;
            let start = self.motion.now;
            self.motion.active.insert(id, Active { tween, start });
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

    /// Starts, retargets or snaps an offset. Without an offset timing the
    /// offset snaps; a policy's snap (reduced motion, hidden, zero duration)
    /// still completes.
    pub fn transition_offset(&mut self, id: NodeId, target: Point) -> Result {
        let (policy, timing) = self.timing(id, TransitionProperty::Offset);
        let current = self.tree.get(id).unwrap().context.offset;
        match plan(&mut self.motion.moving, id, current, target, timing)? {
            Step::Unchanged => {}
            Step::Started => self.repaint = true,
            Step::Snapped => {
                self.tree.get_mut(id).unwrap().context.offset = target;
                self.geometry_dirty = true;
                self.repaint = true;
                if policy {
                    self.complete(id);
                }
            }
        }
        Ok(())
    }

    /// Starts, retargets or snaps scale and rotation, each with its own timing,
    /// like [`Self::transition_offset`].
    pub fn transition_spin(&mut self, id: NodeId, target: Transform) -> Result {
        let (scale_policy, scale_timing) = self.timing(id, TransitionProperty::Scale);
        let (turn_policy, turn_timing) = self.timing(id, TransitionProperty::Rotation);
        let mut spin = self.tree.get(id).unwrap().context.spin;
        let motion = &mut self.motion;
        let scale = plan(
            &mut motion.scaling,
            id,
            spin.scale,
            target.scale,
            scale_timing,
        )?;
        let turn = plan(
            &mut motion.rotating,
            id,
            spin.rotation,
            target.rotation,
            turn_timing,
        )?;
        if scale == Step::Started || turn == Step::Started {
            self.repaint = true;
        }
        if scale == Step::Snapped {
            spin.scale = target.scale;
        }
        if turn == Step::Snapped {
            spin.rotation = target.rotation;
        }
        if scale == Step::Snapped || turn == Step::Snapped {
            self.set_spin(id, spin);
        }
        if (scale == Step::Snapped && scale_policy) || (turn == Step::Snapped && turn_policy) {
            self.complete(id);
        }
        Ok(())
    }

    /// The logical target scale and rotation.
    pub fn target_spin(&self, id: NodeId) -> Transform {
        let spin = self.tree.get(id).unwrap().context.spin;
        Transform {
            scale: self
                .motion
                .scaling
                .get(&id)
                .map_or(spin.scale, |r| r.tween.target()),
            rotation: self
                .motion
                .rotating
                .get(&id)
                .map_or(spin.rotation, |r| r.tween.target()),
        }
    }

    /// Jumps a running scale/rotation to its target. Returns whether one was running.
    pub fn snap_spin(&mut self, id: NodeId) -> bool {
        let target = self.target_spin(id);
        let scaled = self.motion.scaling.remove(&id).is_some();
        let turned = self.motion.rotating.remove(&id).is_some();
        if scaled || turned {
            self.set_spin(id, target);
        }
        scaled || turned
    }

    /// Starts geometric transitions requested since the last refresh.
    pub fn start_offsets(&mut self) {
        let now = self.motion.now;
        for moving in self.motion.moving.values_mut() {
            moving.start.get_or_insert(now);
        }
        let spins = self.motion.scaling.values_mut();
        for running in spins.chain(self.motion.rotating.values_mut()) {
            running.start.get_or_insert(now);
        }
        if let Some(fling) = &mut self.motion.fling {
            fling.last.get_or_insert(now);
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
        if !self.motion.running(id)
            && let Some(handler) = self.motion.ends.get(&id)
        {
            self.pending.push_back((id, handler.version));
        }
    }

    pub fn advance_animations(&mut self, now: Duration) -> Result {
        if now < self.motion.now {
            return Err(UiError::InvalidValue.into());
        }
        self.motion.now = now;
        self.step_fling(now)?;
        let Motion {
            moving,
            scaling,
            rotating,
            active,
            tracks,
            finished,
            ..
        } = &mut self.motion;
        let tree = &mut self.tree;
        let mut moved = advance(moving, tree, now, finished, |e, v| e.offset = v);
        moved |= advance(scaling, tree, now, finished, |e, v| e.spin.scale = v);
        moved |= advance(rotating, tree, now, finished, |e, v| e.spin.rotation = v);
        self.geometry_dirty |= moved;
        self.repaint |= moved;
        active.retain(|id, active| {
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
            let done = active.tween.finished(elapsed);
            if done {
                finished.push(*id);
            }
            !done
        });
        let mut finished = std::mem::take(&mut self.motion.finished);
        for (index, &id) in finished.iter().enumerate() {
            // A node finishing several properties at once completes once.
            if !finished[..index].contains(&id) {
                self.complete(id);
            }
        }
        finished.clear();
        self.motion.finished = finished;
        Ok(())
    }
}
