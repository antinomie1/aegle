use crate::{Appearance, Result, Transition, UiError, state::State};
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
}

pub(crate) struct Track {
    pub timing: Transition,
    pub presented: Option<Appearance>,
}

pub(crate) struct Active {
    tween: Tween<Paint>,
    start: Duration,
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
            self.motion.active.remove(&id);
            track.presented = Some(target);
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
            self.motion.active.remove(&id);
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

    pub fn advance_animations(&mut self, now: Duration) -> Result {
        if now < self.motion.now {
            return Err(UiError::InvalidValue.into());
        }
        self.motion.now = now;
        let tracks = &mut self.motion.tracks;
        let tree = &mut self.tree;
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
            !active.tween.finished(elapsed)
        });
        Ok(())
    }
}
