use aegle_core::NodeId;
use aegle_types::Point;

use crate::{Result, state::State};

/// Tree-wide behavior a control library adds to the engine, as plain function
/// pointers so a hook can run while the engine is borrowed and call back into it.
/// Install with [`State::install`]; every field is optional.
#[derive(Default)]
pub struct Hooks {
    /// A key press before focus traversal; returns whether it was used.
    pub key: Option<fn(&mut State, &aegle_controls::KeyInput<'_>) -> Result<bool>>,
    /// A primary press at a window point, before it is routed.
    pub press: Option<fn(&mut State, Point) -> Result>,
    /// The overlay node covering a window point, which blocks hits below it.
    pub overlay_at: Option<fn(&State, Point) -> Option<NodeId>>,
    /// After geometry: moves overlays; returns whether anything moved.
    pub place: Option<fn(&mut State) -> bool>,
    /// A node was removed (called for it and each descendant, children first, after the
    /// subtree is destroyed). The id is dead: use it only as the key of library data
    /// and never query the tree with it.
    pub removed: Option<fn(&mut State, NodeId)>,
    /// A subtree was removed.
    pub removed_after: Option<fn(&mut State) -> Result>,
    /// After layout: measures realized content; returns whether anything moved.
    pub measure: Option<fn(&mut State) -> Result<bool>>,
    /// Before refresh, outside any engine borrow: builds or drops virtual content.
    pub realize: Option<fn(&crate::Ui) -> Result<bool>>,
    /// The hovered control changed (to `None` when the pointer left).
    pub hover: Option<fn(&mut State, Option<NodeId>) -> Result>,
    /// [`State::wake`] passed: delayed work such as showing a tooltip.
    pub wake: Option<fn(&mut State, std::time::Instant) -> Result>,
}
