//! Taffy layout over the same retained topology used by application state.
//!
//! There is no second owned layout tree. Default styles are shared and leaf
//! measurement is supplied by the host, so this crate needs no text or GPU stack.

mod adapter;
#[cfg(feature = "grid")]
mod grid;
mod node;
#[cfg(feature = "grid")]
mod template;
mod values;

#[cfg(feature = "grid")]
pub use grid::{Flow, Placement, Track};
pub use node::LayoutNode;
/// Inline direction of a node, read by flex, grid, block and leaf layout.
pub use taffy::Direction as LayoutDirection;
pub use taffy::geometry::{Rect as Edges, Size};
pub use taffy::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, LengthPercentage,
    LengthPercentageAuto, Overflow, Position, Style,
};
#[cfg(feature = "grid")]
pub use taffy::{
    GridPlacement, GridTemplateAreas, GridTemplateComponent, Line, TrackSizingFunction,
};
#[cfg(feature = "grid")]
pub use template::{GridLine, GridLines, Repeat, Template, TemplateItem, areas, template};
pub use values::{Align, Direction, Insets, Justify, Length, Wrap};

use aegle_core::{Dirty, NodeId, Tree, TreeError};

/// Computes logical, unrounded geometry using the retained tree's cache.
///
/// `measure` returns a leaf's content size. It must return the same result
/// while its context is unchanged. Update context through
/// `Tree::update(id, Dirty::LAYOUT, ...)` to invalidate it. Changes to pixels
/// alone need not invalidate layout. Leaves have no baseline here; see
/// [`compute_with_baselines`]. Taffy `calc` values must come from
/// [`Length::Calc`]; other handles are read as packed numbers, never dereferenced.
pub fn compute<C>(
    tree: &mut Tree<LayoutNode<C>>,
    root: NodeId,
    available: Size<AvailableSpace>,
    measure: impl FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
) -> Result<(), TreeError> {
    compute_with_baselines(tree, root, available, measure, |_, _, _| None)
}

/// [`compute`], where `baseline` gives a leaf's first text baseline from the
/// top of its final border box, for `Align::Baseline`; `None` aligns its
/// bottom edge, as for a leaf without text.
pub fn compute_with_baselines<C>(
    tree: &mut Tree<LayoutNode<C>>,
    root: NodeId,
    available: Size<AvailableSpace>,
    measure: impl FnMut(NodeId, &mut C, Size<Option<f32>>, Size<AvailableSpace>) -> Size<f32>,
    baseline: impl FnMut(NodeId, &C, Size<f32>) -> Option<f32>,
) -> Result<(), TreeError> {
    tree.get(root).ok_or(TreeError::DeadNode)?;
    let mut adapter = adapter::Adapter {
        tree,
        measure: (measure, baseline),
        flat: Default::default(),
    };
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

/// Makes a node transparent to layout, like CSS `display: contents`: its
/// children are laid out as children of its nearest non-contents ancestor,
/// in tree order, and its own style is ignored. The node itself gets that
/// ancestor's size at offset zero, so child positions stay relative to it.
/// A hidden (`Display::None`) contents node hides its children.
pub fn set_contents<C>(
    tree: &mut Tree<LayoutNode<C>>,
    id: NodeId,
    contents: bool,
) -> Result<(), TreeError> {
    tree.update(id, Dirty::ALL, |node| node.contents = contents)
}
