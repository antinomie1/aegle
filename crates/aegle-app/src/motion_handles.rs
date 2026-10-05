use crate::{Node, Result, Style, Transition, Ui, UiError, motion::Track, state::Content};
use aegle_core::Dirty;
use std::time::Duration;

impl Node {
    /// Animates future changes to this control's paint values as one transition.
    /// Layout, font size and control state are not delayed. Retargeting starts
    /// from the last sampled presentation, including theme and skin changes.
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
    /// Removes the transition policy and immediately returns to the logical skin target.
    pub fn clear_transition(&self) -> Result {
        self.change(|state, id| {
            state.motion.active.remove(&id);
            state.motion.tracks.remove(&id);
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Last sampled appearance; [`Self::appearance`] remains the logical target.
    pub fn presented_appearance(&self) -> Result<crate::Appearance> {
        self.change(|state, id| state.presented_appearance(id))
    }
    /// Whether the most recent refresh started a transition that is still running.
    pub fn is_animating(&self) -> Result<bool> {
        self.change(|state, id| Ok(state.motion.active.contains_key(&id)))
    }
    /// Jumps to the current logical target, retaining timing for future changes.
    pub fn finish_transition(&self) -> Result {
        self.change(|state, id| {
            let target = state.appearance(id)?;
            state.motion.active.remove(&id);
            if let Some(track) = state.motion.tracks.get_mut(&id) {
                track.presented = Some(target);
            }
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Stops at the sampled appearance and freezes it into local paint overrides.
    /// State-specific local colors are replaced. Focus visibility still follows
    /// behavior; timing remains for future setters.
    pub fn cancel_transition(&self) -> Result {
        self.change(|state, id| {
            let value = state.presented_appearance(id)?;
            let content = &state.tree.get(id).unwrap().context.content;
            let field = matches!(content, Content::Field(_));
            let control = content.interactive();
            let indicator = matches!(
                content,
                Content::Toggle(_) | Content::Slider(_) | Content::Progress(_)
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
        !self.state.borrow().motion.active.is_empty()
    }
    /// Explicit reduced-motion preference. When true all paint transitions snap
    /// to their targets and no new animations start. OS preference discovery is
    /// separate; changing this flag does not affect editor/focus state.
    pub fn set_reduced_motion(&self, reduced: bool) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        if state.motion.reduced == reduced {
            return Ok(());
        }
        state.motion.reduced = reduced;
        let ids: Vec<_> = state.motion.active.keys().copied().collect();
        for id in ids {
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
        if reduced {
            let repaint = state.refresh()?;
            // Keep the snapped frame pending for the host's next presentation.
            state.repaint |= repaint;
        }
        Ok(())
    }
}
