use crate::{
    Node, Result, State, Style, Transition, TransitionProperty, Ui, UiError, motion::Track,
    tokens::TokenSlot,
};
use aegle_core::{Dirty, NodeId};
use std::time::Duration;

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
            let moved = state.snap_offset(id);
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
            let value = state.presented_appearance(id)?;
            let control = &state.tree.get(id).unwrap().context.control;
            let scope = crate::control::StyleScope::of(control.kind());
            let (field, indicator, control) =
                (scope.editor, scope.indicator, control.interactive());
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
            timings: [None; 4],
            presented: Some(current),
        });
        track.timings[property as usize] = timing;
        if track.timings == [None; 4] {
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
            _ => {}
        }
        Ok(())
    }
}
