use crate::{
    Node, Point, Result, State, Style, Transform, Transition, TransitionProperty, Ui, UiError,
    motion::{Running, Track},
    tokens::TokenSlot,
};
use aegle_core::{Dirty, NodeId};
use aegle_motion::Animation;
use std::time::Duration;

/// An explicit animation of one presented property, see [`Node::animate`].
#[derive(Clone, Debug)]
pub enum Animate {
    /// The translation `Node::set_offset` sets.
    Offset(Animation<Point>),
    /// The scale of `Node::set_transform`; overshoot stays positive.
    Scale(Animation<f32>),
    /// The rotation of `Node::set_transform`, in radians.
    Rotation(Animation<f32>),
    /// The group opacity of `Node::set_opacity`, clamped to `0..=1`.
    Opacity(Animation<f32>),
}

impl Node {
    /// Animates future changes to this control's paint values, offset, scale
    /// and rotation with one timing. Layout, font size and control state are
    /// not delayed. Retargeting starts from the last sampled presentation,
    /// including theme and skin changes. A running geometric transition keeps
    /// the timing it started with. Ends duration token bindings.
    pub fn set_transition(&self, timing: Transition) -> Result {
        self.change(|state, id| {
            let current = state.presented_appearance(id)?;
            state.motion.active.remove(&id);
            let track = Track {
                presented: Some(current),
                ..Track::uniform(timing)
            };
            state.motion.tracks.insert(id, track);
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            state.tokens.unbind(id, TokenSlot::is_transition);
            Ok(())
        })
    }
    /// Sets the timing of one property, keeping the others; `None` makes that
    /// property change immediately, jumping a running transition of it to its
    /// target without a completion callback. A control without any policy
    /// starts with every other property immediate. Ends a token binding of
    /// the property's duration.
    pub fn set_property_transition(
        &self,
        property: TransitionProperty,
        timing: Option<Transition>,
    ) -> Result {
        self.change(|state, id| {
            state.set_property_transition(id, property, timing)?;
            state
                .tokens
                .unbind(id, |s| s == TokenSlot::Transition(property));
            Ok(())
        })
    }
    /// The timing of one property, if it animates.
    pub fn property_transition(&self, property: TransitionProperty) -> Result<Option<Transition>> {
        self.change(|state, id| {
            Ok(state
                .motion
                .tracks
                .get(&id)
                .and_then(|track| track.timing(property)))
        })
    }
    /// Removes the transition policy and immediately returns to the logical
    /// targets without a completion callback. Ends duration token bindings.
    pub fn clear_transition(&self) -> Result {
        self.change(|state, id| {
            state.motion.active.remove(&id);
            state.motion.tracks.remove(&id);
            state.snap_offset(id);
            state.snap_spin(id);
            state.snap_opacity(id);
            state.snap_shadow(id);
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            state.tokens.unbind(id, TokenSlot::is_transition);
            Ok(())
        })
    }
    /// Last sampled appearance; [`Self::appearance`] remains the logical target.
    pub fn presented_appearance(&self) -> Result<crate::Appearance> {
        self.change(|state, id| state.presented_appearance(id))
    }
    /// Whether a paint transition started by the most recent refresh, or a
    /// geometric transition, is still running.
    pub fn is_animating(&self) -> Result<bool> {
        self.change(|state, id| Ok(state.motion.running(id)))
    }
    /// Jumps to the current logical targets, retaining timing for future changes.
    /// Completes a running transition.
    pub fn finish_transition(&self) -> Result {
        self.change(|state, id| {
            let target = state.appearance(id)?;
            let painting = state.motion.active.remove(&id).is_some();
            if let Some(track) = state.motion.tracks.get_mut(&id) {
                track.presented = Some(target);
            }
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            let moved = state.snap_offset(id) | state.snap_opacity(id) | state.snap_shadow(id);
            if state.snap_spin(id) || moved || painting {
                state.complete(id);
            }
            Ok(())
        })
    }
    /// Adds a handler run after this control's transitions all reach their
    /// targets, by normal completion, [`Self::finish_transition`], or a
    /// policy change that snaps (reduced motion, hidden, zero duration).
    /// Cancelling, clearing the policy and removal do not complete. Dispatched
    /// like click handlers, outside every UI borrow, in registration order.
    pub fn on_transition_end(&self, callback: impl FnMut(Node) -> Result + 'static) -> Result {
        self.change(|state, id| {
            let callback = Box::new(callback);
            crate::callbacks::add(
                &mut state.motion.ends,
                &mut state.callback_version,
                id,
                callback,
            )
        })
    }
    /// Removes the completion handlers and any queued invocation.
    pub fn clear_on_transition_end(&self) -> Result {
        self.change(|state, id| {
            state.motion.ends.remove(&id);
            Ok(())
        })
    }
    /// Stops at the sampled appearance and offset, freezing them into local
    /// paint overrides and the offset target. State-specific local colors are
    /// replaced and style token bindings end. Focus visibility still follows
    /// behavior; timing remains for future setters.
    pub fn cancel_transition(&self) -> Result {
        self.change(|state, id| {
            state.motion.moving.remove(&id);
            state.motion.scaling.remove(&id);
            state.motion.rotating.remove(&id);
            state.motion.fading.remove(&id);
            state.motion.shadows.remove(&id);
            let value = state.presented_appearance(id)?;
            let control = &state.tree.get(id).unwrap().context.control;
            let accepts = control.kind().accepts;
            let (field, indicator, control) = (
                accepts.contains(aegle_theme::Accepts::EDITOR),
                accepts.contains(aegle_theme::Accepts::INDICATOR),
                accepts.contains(aegle_theme::Accepts::INTERACTIVE),
            );
            state.set_style(
                id,
                Style {
                    background: Some(value.background),
                    foreground: Some(value.foreground),
                    border_color: Some(value.border_color),
                    border_width: Some(value.border_width),
                    radius: Some(value.radius),
                    focus_color: control.then_some(value.focus_color),
                    focus_width: control.then_some(value.focus_width),
                    selection: field.then_some(value.selection),
                    caret: field.then_some(value.caret),
                    indicator: indicator.then_some(value.indicator),
                    ..Default::default()
                },
            )?;
            state.tokens.unbind(id, TokenSlot::is_style);
            let target = state.appearance(id)?;
            if let Some(track) = state.motion.tracks.get_mut(&id) {
                track.presented = Some(target);
            }
            state.motion.active.remove(&id);
            Ok(())
        })
    }
}

impl Node {
    /// Runs `change`, animating the paint, offset, scale and rotation changes
    /// it makes to this control with `timing` instead of the control's
    /// transition policy, which stays unchanged. Changes to other controls
    /// follow their own policy. Reduced motion still snaps.
    ///
    /// ```ignore
    /// card.with_transition(Transition::spring(Spring::new(300.0, 12.0)?), || {
    ///     card.set_offset(Point::new(0.0, 24.0))
    /// })?;
    /// ```
    pub fn with_transition<R>(
        &self,
        timing: Transition,
        change: impl FnOnce() -> Result<R>,
    ) -> Result<R> {
        self.scoped(Some(timing), change)
    }
    /// Runs `change` with the changes it makes to this control applied at
    /// once, without a transition, whatever the policy. A transition already
    /// running for a changed property jumps to the new value and completes.
    pub fn snap<R>(&self, change: impl FnOnce() -> Result<R>) -> Result<R> {
        self.scoped(None, change)
    }
    fn scoped<R>(
        &self,
        timing: Option<Transition>,
        change: impl FnOnce() -> Result<R>,
    ) -> Result<R> {
        let outer = self.change(|state, id| {
            if timing.is_some() && !state.motion.tracks.contains_key(&id) {
                // A policy-free control animates from what it shows now.
                let presented = Some(state.appearance(id)?);
                let timings = [None; TransitionProperty::ALL.len()];
                state.motion.tracks.insert(id, Track { timings, presented });
            }
            state.motion.paint_once.insert(id, timing);
            Ok(state.motion.scoped.replace((id, timing)))
        })?;
        let result = change();
        // The closure returned, so nothing borrows the UI; it may have
        // removed this control or dropped the UI.
        if let Some(owner) = self.state.upgrade() {
            owner.borrow_mut().motion.scoped = outer;
        }
        result
    }
    /// Starts an explicit animation of one property from its first
    /// keyframe, replacing a running transition or animation of it. When it
    /// finishes the property rests at its [`Animation::target`], which also
    /// becomes the logical value (`offset`, `transform`, `opacity`). It shares the
    /// transition lifecycle: [`Self::is_animating`], [`Self::finish_transition`],
    /// [`Self::cancel_transition`] and [`Self::on_transition_end`]. Reduced
    /// motion and hidden controls go straight to the target; a
    /// [`Cycles::Forever`](aegle_motion::Cycles::Forever) animation runs, and
    /// requests frames, until stopped.
    pub fn animate(&self, animation: Animate) -> Result {
        self.change(|state, id| {
            let element = &mut state.tree.get_mut(id).unwrap().context;
            match animation {
                Animate::Offset(curve) => {
                    element.offset = curve.sample(Duration::ZERO);
                    state.motion.moving.insert(id, Running::new(curve));
                }
                Animate::Scale(curve) => {
                    let spin = Transform {
                        scale: curve.sample(Duration::ZERO).max(f32::EPSILON),
                        ..element.spin
                    };
                    state.set_spin(id, spin);
                    state.motion.scaling.insert(id, Running::new(curve));
                }
                Animate::Rotation(curve) => {
                    let spin = Transform {
                        rotation: curve.sample(Duration::ZERO),
                        ..element.spin
                    };
                    state.set_spin(id, spin);
                    state.motion.rotating.insert(id, Running::new(curve));
                }
                Animate::Opacity(curve) => {
                    element.group.opacity = curve.sample(Duration::ZERO).clamp(0.0, 1.0);
                    state.groups.entry(id).or_default();
                    state.motion.fading.insert(id, Running::new(curve));
                }
            }
            state.geometry_dirty = true;
            state.repaint = true;
            if state.motion.reduced {
                state.snap_offset(id);
                state.snap_spin(id);
                state.snap_opacity(id);
                state.complete(id);
            }
            Ok(())
        })
    }
}

impl Ui {
    /// Sets timing for subsequently created interactive controls. Existing policies
    /// are unchanged. Headless UIs default to None; native App opts into 120 ms.
    pub fn set_default_transition(&self, timing: Option<Transition>) -> Result {
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .motion
            .default = timing;
        Ok(())
    }
    /// Samples active paint transitions at a caller-owned monotonic timestamp.
    /// Start at any nonnegative time; moving backwards is rejected. Then refresh
    /// and present normally. No active animation means no work beyond clock update.
    pub fn advance_animations(&self, now: Duration) -> Result {
        self.state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?
            .advance_animations(now)
    }
    /// Whether a host must request another frame. The Ui owns no timer or thread.
    pub fn has_animations(&self) -> Result<bool> {
        let state = self.read()?;
        Ok(state.motion.any_running() || state.motion.fling.is_some())
    }
    /// Whether reduced motion is currently in effect.
    pub fn reduced_motion(&self) -> Result<bool> {
        Ok(self.read()?.motion.reduced)
    }
    /// Explicit reduced-motion preference. When true all transitions snap to
    /// their targets, completing, and no new animations start. Changing this
    /// flag does not affect editor/focus state.
    pub fn set_reduced_motion(&self, reduced: bool) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if state.motion.reduced == reduced {
            return Ok(());
        }
        state.motion.reduced = reduced;
        if reduced {
            state.motion.fling = None;
        }
        let ids: Vec<_> = state.motion.active.keys().copied().collect();
        for id in ids {
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
        if reduced {
            let moving: Vec<_> = state.motion.moving.keys().copied().collect();
            for id in moving {
                state.snap_offset(id);
                state.complete(id);
            }
            let spins = state
                .motion
                .scaling
                .keys()
                .chain(state.motion.rotating.keys());
            let turning: std::collections::HashSet<_> = spins.copied().collect();
            for id in turning {
                state.snap_spin(id);
                state.complete(id);
            }
            let fading: Vec<_> = state.motion.fading.keys().copied().collect();
            for id in fading {
                state.snap_opacity(id);
                state.complete(id);
            }
            let shadows: Vec<_> = state.motion.shadows.keys().copied().collect();
            for id in shadows {
                state.snap_shadow(id);
                state.complete(id);
            }
            let repaint = state.refresh()?;
            // Keep the snapped frame pending for the host's next presentation.
            state.repaint |= repaint;
        }
        Ok(())
    }
}

impl State {
    /// [`Node::set_property_transition`] without ending a binding.
    pub fn set_property_transition(
        &mut self,
        id: NodeId,
        property: TransitionProperty,
        timing: Option<Transition>,
    ) -> Result {
        let current = self.presented_appearance(id)?;
        let track = self.motion.tracks.entry(id).or_insert(Track {
            timings: [None; TransitionProperty::ALL.len()],
            presented: Some(current),
        });
        track.timings[property as usize] = timing;
        if track.timings == [None; TransitionProperty::ALL.len()] {
            self.motion.tracks.remove(&id);
        }
        match property {
            TransitionProperty::Paint => {
                self.motion.active.remove(&id);
                self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            TransitionProperty::Offset if timing.is_none() => {
                self.snap_offset(id);
            }
            TransitionProperty::Scale | TransitionProperty::Rotation if timing.is_none() => {
                let target = self.target_spin(id);
                let mut spin = self.tree.get(id).unwrap().context.spin;
                if property == TransitionProperty::Scale {
                    self.motion.scaling.remove(&id);
                    spin.scale = target.scale;
                } else {
                    self.motion.rotating.remove(&id);
                    spin.rotation = target.rotation;
                }
                self.set_spin(id, spin);
            }
            TransitionProperty::Opacity if timing.is_none() => {
                self.snap_opacity(id);
            }
            TransitionProperty::Shadow if timing.is_none() => {
                self.snap_shadow(id);
            }
            _ => {}
        }
        Ok(())
    }
}

impl State {
    /// Whether a `Node::snap` closure is changing `id`, for control
    /// libraries' own value animations.
    pub fn snapping(&self, id: NodeId) -> bool {
        self.motion.scoped == Some((id, None))
    }
    /// Ends the paint timing of `with_transition` and `snap` closures once
    /// their changes were recorded.
    pub(crate) fn settle_scoped(&mut self) {
        if self.motion.paint_once.is_empty() {
            return;
        }
        let scoped: Vec<_> = self.motion.paint_once.drain().map(|(id, _)| id).collect();
        for id in scoped {
            self.release_track(id);
        }
    }
    /// Drops the policy-free track a `with_transition` closure created once
    /// its paint transition is over.
    pub(crate) fn release_track(&mut self, id: NodeId) {
        let idle = !self.motion.active.contains_key(&id);
        if idle
            && self
                .motion
                .tracks
                .get(&id)
                .is_some_and(|t| t.timings == [None; TransitionProperty::ALL.len()])
        {
            self.motion.tracks.remove(&id);
        }
    }
}
