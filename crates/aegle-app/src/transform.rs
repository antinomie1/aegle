use crate::{Node, Result, UiError, state::State};
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
    pub fn set_transform(&self, transform: Transform) -> Result {
        if !transform.valid() {
            return Err(UiError::InvalidValue.into());
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
    pub fn transform(&self) -> Result<Transform> {
        self.change(|state, id| {
            #[cfg(feature = "motion")]
            if let Some(turning) = state.motion.turning.get(&id) {
                return Ok(turning.tween.target().0);
            }
            Ok(state.tree.get(id).unwrap().context.spin)
        })
    }
}
