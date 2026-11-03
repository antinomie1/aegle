//! Which corners of a node take its radius.

use crate::{Node, Result};
use aegle_core::Dirty;

/// The corners of a node its radius rounds; the others stay square. A bar
/// along a screen edge, for example, rounds only the corners away from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Corners {
    /// The top-left corner.
    pub top_left: bool,
    /// The top-right corner.
    pub top_right: bool,
    /// The bottom-right corner.
    pub bottom_right: bool,
    /// The bottom-left corner.
    pub bottom_left: bool,
}

impl Corners {
    /// Every corner, the default.
    pub const ALL: Self = Self {
        top_left: true,
        top_right: true,
        bottom_right: true,
        bottom_left: true,
    };
    /// No corner.
    pub const NONE: Self = Self {
        top_left: false,
        top_right: false,
        bottom_right: false,
        bottom_left: false,
    };

    /// `radius` at these corners and zero at the others, in the order
    /// [`aegle_scene::RoundedRect::with_radii`] takes.
    pub fn radii(self, radius: f32) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
        .map(|on| if on { radius } else { 0.0 })
    }
}

impl Default for Corners {
    fn default() -> Self {
        Self::ALL
    }
}

impl Node {
    /// Rounds only `corners` with this node's radius: its background,
    /// border, focus ring, shadow, group layer and backdrop blur follow.
    pub fn set_corners(&self, corners: Corners) {
        self.change(|state, id| -> Result {
            let element = &mut state.tree.get_mut(id).unwrap().context;
            if element.corners != corners {
                element.corners = corners;
                state.geometry_dirty = true;
                state.invalidate(id, Dirty::PAINT)?;
            }
            Ok(())
        })
    }

    /// The corners this node's radius rounds.
    pub fn corners(&self) -> Corners {
        self.change(|state, id| Ok(state.tree.get(id).unwrap().context.corners))
    }
}
