use crate::{Node, UiError, state::State};
#[cfg(feature = "motion")]
use crate::{
    TransitionProperty,
    motion::{Running, Step, plan},
};
use aegle_core::NodeId;

/// Scale and rotation applied about a node's center, inherited by its subtree.
///
/// Presentation only: layout, scroll extents and bounds are unchanged. Scenes,
/// pointer hit testing, scroll-view clipping (to the transformed bounding box),
/// the IME anchor and accessibility transforms follow it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Uniform scale, finite and strictly positive.
    pub scale: f32,
    /// Clockwise rotation in radians, finite.
    pub rotation: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            scale: 1.0,
            rotation: 0.0,
        }
    }
}

impl Transform {
    pub(crate) fn valid(self) -> bool {
        self.scale.is_finite()
            && self.scale > 0.0
            && self.scale <= 1_000.0
            && self.rotation.is_finite()
    }
}

impl State {
    pub(crate) fn set_spin(&mut self, id: NodeId, spin: Transform) {
        self.tree.get_mut(id).unwrap().context.spin = spin;
        self.geometry_dirty = true;
        self.repaint = true;
        self.ime_dirty = true;
    }
}

impl Node {
    /// Sets the scale and rotation of this subtree. With `motion`, a control with
    /// a transition policy animates from its presented value.
    pub fn set_transform(&self, transform: Transform) {
        if !transform.valid() {
            panic!("{}", UiError::InvalidValue);
        }
        self.change(|state, id| {
            #[cfg(feature = "motion")]
            return state.transition_spin(id, transform);
            #[cfg(not(feature = "motion"))]
            {
                state.set_spin(id, transform);
                Ok(())
            }
        })
    }

    /// The logical target transform; scenes show the presented one.
    pub fn transform(&self) -> Transform {
        self.change(|state, id| {
            #[cfg(feature = "motion")]
            return Ok(state.target_spin(id));
            #[cfg(not(feature = "motion"))]
            Ok(state.tree.get(id).unwrap().context.spin)
        })
    }
}

#[cfg(feature = "motion")]
impl State {
    /// Starts, retargets or snaps scale and rotation, each with its own timing,
    /// like [`Self::transition_offset`].
    pub fn transition_spin(&mut self, id: NodeId, target: Transform) -> crate::Result {
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
                .map_or(spin.scale, Running::target),
            rotation: self
                .motion
                .rotating
                .get(&id)
                .map_or(spin.rotation, Running::target),
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
}
