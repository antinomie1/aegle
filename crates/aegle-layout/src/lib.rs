//! Taffy layout over the same retained topology used by application state.
//!
//! There is no second owned layout tree. Default styles are shared and leaf
//! measurement is supplied by the host, so this crate needs no text or GPU stack.
//! The public API is Aegle's own: [`LayoutStyle`] and the value types it takes;
//! Taffy's types stay inside, so a Taffy upgrade changes no caller.

mod adapter;
#[cfg(feature = "grid")]
mod grid;
mod item;
mod node;
mod style;
#[cfg(feature = "grid")]
mod template;
mod values;

#[cfg(feature = "grid")]
pub use grid::{Flow, Placement, Track};
pub use node::LayoutNode;
pub use style::{Display, LayoutDirection, LayoutStyle, Overflow};
#[cfg(feature = "grid")]
pub use template::{GridLine, GridLines, Repeat, TemplateItem};
pub use values::{Align, Direction, Insets, Justify, Length, Wrap};

use aegle_core::{Dirty, NodeId, Tree, TreeError};

/// Space offered to a node on one axis while it is measured.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Available {
    /// This many logical pixels.
    Definite(f32),
    /// As little as the content allows, such as its longest word.
    MinContent,
    /// As much as the content wants, without wrapping.
    MaxContent,
}

/// Space offered on both axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Space {
    /// Horizontal space.
    pub width: Available,
    /// Vertical space.
    pub height: Available,
}

impl Space {
    /// Unconstrained on both axes.
    pub const MAX_CONTENT: Self = Self {
        width: Available::MaxContent,
        height: Available::MaxContent,
    };

    /// Exactly `size`, such as a window's.
    pub fn definite(size: aegle_types::Size) -> Self {
        Self {
            width: Available::Definite(size.width),
            height: Available::Definite(size.height),
        }
    }
}

/// Sizes the layout has already decided for a leaf it measures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Known {
    /// A decided width.
    pub width: Option<f32>,
    /// A decided height.
    pub height: Option<f32>,
}

/// Computes logical, unrounded geometry using the retained tree's cache.
///
/// `measure` returns a leaf's content size. It must return the same result
/// while its context is unchanged. Update context through
/// `Tree::update(id, Dirty::LAYOUT, ...)` to invalidate it. Changes to pixels
/// alone need not invalidate layout. Leaves have no baseline here; see
/// [`compute_with_baselines`].
///
/// Two rules depart from CSS so content scrolls instead of growing the
/// window: an axis with [`Overflow::Scroll`] and no definite size measures,
/// under a min-content constraint, as only its padding, border and minimum
/// size; and the items of a flex container that scrolls along its main axis
/// do not shrink.
pub fn compute<C>(
    tree: &mut Tree<LayoutNode<C>>,
    root: NodeId,
    available: Space,
    measure: impl FnMut(NodeId, &mut C, Known, Space) -> aegle_types::Size,
) -> Result<(), TreeError> {
    compute_with_baselines(tree, root, available, measure, |_, _, _| None)
}

/// [`compute`], where `baseline` gives a leaf's first text baseline from the
/// top of its final border box, for `Align::Baseline`; `None` aligns its
/// bottom edge, as for a leaf without text.
pub fn compute_with_baselines<C>(
    tree: &mut Tree<LayoutNode<C>>,
    root: NodeId,
    available: Space,
    measure: impl FnMut(NodeId, &mut C, Known, Space) -> aegle_types::Size,
    baseline: impl FnMut(NodeId, &mut C, aegle_types::Size) -> Option<f32>,
) -> Result<(), TreeError> {
    tree.get(root).ok_or(TreeError::DeadNode)?;
    let mut adapter = adapter::Adapter {
        tree,
        measure: (measure, baseline),
        flat: Default::default(),
    };
    let axis = |space| match space {
        Available::Definite(value) => taffy::AvailableSpace::Definite(value),
        Available::MinContent => taffy::AvailableSpace::MinContent,
        Available::MaxContent => taffy::AvailableSpace::MaxContent,
    };
    let available = taffy::Size {
        width: axis(available.width),
        height: axis(available.height),
    };
    taffy::compute_root_layout(&mut adapter, adapter::to_taffy(root), available);
    Ok(())
}

/// Replaces style and invalidates this node and its ancestors.
pub fn set_style<C>(
    tree: &mut Tree<LayoutNode<C>>,
    id: NodeId,
    style: LayoutStyle,
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
