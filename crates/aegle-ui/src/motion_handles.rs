use crate::{Node, Result, Style, Transition, Ui, UiError, callbacks::Handler, motion::Track};
use aegle_core::Dirty;
use std::time::Duration;

impl Node {
    /// Animates future changes to this control's paint values and offset.
    /// Layout, font size and control state are not delayed. Retargeting starts
    /// from the last sampled presentation, including theme and skin changes.
    /// A running offset keeps the timing it started with.
    pub fn set_transition(&self, timing: Transition) -> Result {
        self.change(|state, id| {
            let current = state.presented_appearance(id)?;
            state.motion.active.remove(&id);
            state.motion.tracks.insert(
                id,
                Track {
                    timing,
                    presented: Some(current),
                },
            );
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Removes the transition policy and immediately returns to the logical
    /// targets without a completion callback.
    pub fn clear_transition(&self) -> Result {
        self.change(|state, id| {
            state.motion.active.remove(&id);
            state.motion.tracks.remove(&id);
            state.snap_offset(id);
            state.snap_spin(id);
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Last sampled appearance; [`Self::appearance`] remains the logical target.
    pub fn presented_appearance(&self) -> Result<crate::Appearance> {
        self.change(|state, id| state.presented_appearance(id))
    }
    /// Whether a paint transition started by the most recent refresh, or an
    /// offset transition, is still running.
    pub fn is_animating(&self) -> Result<bool> {
        self.change(|state, id| {
            Ok(state.motion.active.contains_key(&id)
                || state.motion.moving.contains_key(&id)
                || state.motion.turning.contains_key(&id))
        })
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
    /// Runs after this control's transitions all reach their targets, by normal
    /// completion, [`Self::finish_transition`], or a policy change that snaps
    /// (reduced motion, hidden, zero duration). Cancelling, clearing the policy
    /// and removal do not complete. Dispatched like click handlers, outside
    /// every UI borrow; replaces the previous handler.
    pub fn on_transition_end(&self, callback: impl FnMut(Node) -> Result + 'static) -> Result {
        self.change(|state, id| {
            state.callback_version = state
                .callback_version
                .checked_add(1)
                .ok_or(UiError::IdentityExhausted)?;
            let handler = Handler {
                version: state.callback_version,
                callback: Some(Box::new(callback)),
            };
            state.motion.ends.insert(id, handler);
            Ok(())
        })
    }
    /// Removes the completion handler and any queued invocation.
    pub fn clear_on_transition_end(&self) -> Result {
        self.change(|state, id| {
            state.motion.ends.remove(&id);
            Ok(())
        })
    }
    /// Stops at the sampled appearance and offset, freezing them into local
    /// paint overrides and the offset target. State-specific local colors are
    /// replaced. Focus visibility still follows behavior; timing remains for
    /// future setters.
    pub fn cancel_transition(&self) -> Result {
        self.change(|state, id| {
            state.motion.moving.remove(&id);
            state.motion.turning.remove(&id);
            let value = state.presented_appearance(id)?;
            let control = &state.tree.get(id).unwrap().context.control;
            let scope = control.style_scope();
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
    pub fn has_animations(&self) -> bool {
        let state = self.state.borrow();
        !state.motion.active.is_empty()
            || !state.motion.moving.is_empty()
            || !state.motion.turning.is_empty()
            || state.motion.fling.is_some()
    }
    /// Whether reduced motion is currently in effect.
    pub fn reduced_motion(&self) -> bool {
        self.state.borrow().motion.reduced
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
            let turning: Vec<_> = state.motion.turning.keys().copied().collect();
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
