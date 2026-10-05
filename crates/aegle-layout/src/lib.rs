//! Taffy layout over the same retained topology used by application state.
//!
//! There is no second owned layout tree. Default styles are shared and leaf
//! measurement is supplied by the host, so this crate needs no text or GPU stack.

mod adapter;
mod node;

pub use node::LayoutNode;
pub use taffy::geometry::{Rect as Edges, Size};
pub use taffy::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, LengthPercentage,
    LengthPercentageAuto, Overflow, Position, Style,
};

use aegle_core::{Dirty, NodeId, Tree, TreeError};

/// Computes logical, unrounded geometry using the retained tree's cache.
///
/// A measurement callback must return the same result while its context is
/// unchanged. Update context through `Tree::update(id, Dirty::LAYOUT, ...)` to
/// invalidate it. Changes to pixels alone need not invalidate layout.
/// Taffy `calc` pointer values are not supported.
pub fn compute<C>(
    tree: &mut Tree<LayoutNode<C>>,
    root: NodeId,
    available: Size<AvailableSpace>,
    measure: impl FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
) -> Result<(), TreeError> {
    tree.get(root).ok_or(TreeError::DeadNode)?;
    let mut adapter = adapter::Adapter { tree, measure };
    taffy::compute_root_layout(&mut adapter, adapter::to_taffy(root), available);
    Ok(())
}

/// Replaces style and invalidates this node and its ancestors.
pub fn set_style<C>(
    tree: &mut Tree<LayoutNode<C>>,
    id: NodeId,
    style: Style,
) -> Result<(), TreeError> {
    tree.update(id, Dirty::ALL, |node| node.set_style(style))
}
