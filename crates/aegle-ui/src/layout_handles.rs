//! Layout setters: an item's size, flex and placement on [`Node`], and how a
//! container arranges its children on [`Container`].
//!
//! Sizes are set one axis at a time. Lengths accept logical pixels (`f32`),
//! `Option<f32>` (`None` is automatic) or [`Length`], including percentages of
//! the parent's content box.

use crate::require;
use aegle_core::{Dirty, NodeId};
use aegle_layout::{Align, Direction, Insets, Justify, LayoutStyle, Length, Wrap};

use crate::{Container, LengthSlot, Node, Result, State, UiError};

/// The themed layout values an application set on a node, which a theme
/// change keeps; see [`Control::retheme`](crate::Control::retheme).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalLayout(u8);

impl LocalLayout {
    /// No local values.
    pub const NONE: Self = Self(0);
    /// The height.
    pub const HEIGHT: Self = Self(1);
    /// The padding.
    pub const PADDING: Self = Self(2);
    /// The gap between children.
    pub const GAP: Self = Self(4);
    /// The minimum height.
    pub const MIN_HEIGHT: Self = Self(8);

    /// Whether `value` was set locally.
    pub fn contains(self, value: Self) -> bool {
        self.0 & value.0 == value.0
    }

    pub(crate) fn insert(&mut self, value: Self) {
        self.0 |= value.0;
    }

    pub(crate) fn remove(&mut self, value: Self) {
        self.0 &= !value.0;
    }
}

impl Node {
    pub(crate) fn layout(
        &self,
        local: LocalLayout,
        change: impl FnOnce(&mut LayoutStyle) -> &mut LayoutStyle,
    ) {
        self.change(|state, id| {
            let mut style = state.tree.get(id).unwrap().style().clone();
            change(&mut style);
            state
                .tree
                .get_mut(id)
                .unwrap()
                .context
                .local_layout
                .insert(local);
            aegle_layout::set_style(&mut state.tree, id, style)?;
            Ok(())
        })
    }
    /// Sets the width, preserving height and its theme default.
    pub fn set_width(&self, width: impl Into<Length>) {
        let width = width.into();
        self.layout(LocalLayout::NONE, |s| s.set_width(width))
    }
    /// Sets the height; automatic replaces the control's themed height.
    pub fn set_height(&self, height: impl Into<Length>) {
        let height = height.into();
        self.layout(LocalLayout::HEIGHT, |s| s.set_height(height))
    }
    /// Sets the minimum width without changing the minimum height.
    pub fn set_min_width(&self, width: impl Into<Length>) {
        let width = width.into();
        self.layout(LocalLayout::NONE, |s| s.set_min_width(width))
    }
    /// Sets the minimum height, overriding the corresponding theme default.
    /// Zero lets a flex item shrink below its content. Containers holding a
    /// scroll view or table need none: a scrolling axis does not pass its
    /// content up as a minimum.
    pub fn set_min_height(&self, height: impl Into<Length>) {
        let height = height.into();
        self.layout(LocalLayout::MIN_HEIGHT, |s| s.set_min_height(height))
    }
    /// Sets the maximum width; automatic removes the limit.
    pub fn set_max_width(&self, width: impl Into<Length>) {
        let width = width.into();
        self.layout(LocalLayout::NONE, |s| s.set_max_width(width))
    }
    /// Sets the maximum height; automatic removes the limit.
    pub fn set_max_height(&self, height: impl Into<Length>) {
        let height = height.into();
        self.layout(LocalLayout::NONE, |s| s.set_max_height(height))
    }
    /// Keeps width divided by height at a positive ratio when one dimension
    /// is automatic; `None` removes the constraint.
    pub fn set_aspect_ratio(&self, ratio: impl Into<Option<f32>>) {
        let ratio = ratio.into();
        self.layout(LocalLayout::NONE, |s| s.set_aspect_ratio(ratio))
    }
    /// Sets a finite nonnegative flex grow factor; zero keeps intrinsic sizing.
    pub fn set_grow(&self, grow: f32) {
        self.layout(LocalLayout::NONE, |s| s.set_grow(grow))
    }
    /// Sets how much this item gives up when its line overflows; zero keeps
    /// its basis. The default is one.
    pub fn set_shrink(&self, shrink: f32) {
        self.layout(LocalLayout::NONE, |s| s.set_shrink(shrink))
    }
    /// Sets the main-axis size before growing and shrinking; automatic uses
    /// the size or content. Zero with a grow factor shares space by factor only.
    pub fn set_basis(&self, basis: impl Into<Length>) {
        let basis = basis.into();
        self.layout(LocalLayout::NONE, |s| s.set_basis(basis))
    }
    /// Overrides the parent's cross-axis alignment for this item; in a grid,
    /// its vertical alignment within its area. `None` follows the parent.
    pub fn set_align_self(&self, align: impl Into<Option<Align>>) {
        let align = align.into();
        self.layout(LocalLayout::NONE, |s| s.set_align_self(align))
    }
    /// Sets outer spacing. Edges may be negative; `Auto` edges absorb free
    /// space, so automatic left and right margins center an item.
    pub fn set_margin(&self, margin: impl Into<Insets>) {
        let margin = margin.into();
        self.layout(LocalLayout::NONE, |s| s.set_margin(margin))
    }
    /// Takes this item out of its parent's flow and places it by `insets`
    /// from the parent's padding box, over its siblings; `None` returns it to
    /// the flow. With opposite insets set and an automatic size, it stretches between them.
    pub fn set_absolute(&self, insets: impl Into<Option<Insets>>) {
        let insets = insets.into();
        self.layout(LocalLayout::NONE, |s| s.set_absolute(insets))
    }
    /// Sets inner spacing. Containers accept any [`Insets`]; text-bearing
    /// controls accept only one nonnegative pixel value on every edge.
    /// Ends a padding token binding.
    pub fn set_padding(&self, padding: impl Into<Insets>) {
        let padding = padding.into();
        require(padding.is_valid(true, false));
        self.change(|state, id| {
            state.set_padding(id, Some(padding))?;
            state.tokens.unbind(id, |s| s == LengthSlot::Padding.into());
            Ok(())
        })
    }
    /// Sets the spacing between rows and between columns, in that order like
    /// CSS `gap`. Ends a gap token binding.
    pub fn set_gap(&self, row: impl Into<Length>, column: impl Into<Length>) {
        let (row, column) = (row.into(), column.into());
        require(row != Length::Auto && column != Length::Auto);
        require(row.is_valid(true) && column.is_valid(true));
        self.change(|state, id| {
            state.set_gap(id, Some((row, column)))?;
            state.tokens.unbind(id, |s| s == LengthSlot::Gap.into());
            Ok(())
        })
    }
}

impl State {
    /// Sets checked local padding, or with `None` returns to the control's
    /// themed default.
    pub fn set_padding(&mut self, id: NodeId, padding: Option<Insets>) -> Result {
        let is_root = id == self.root;
        let theme = *self.theme_of(id);
        let node = self.tree.get_mut(id).unwrap();
        if node.context.control.kind().container {
            let mut style = node.style().clone();
            match padding {
                Some(padding) => {
                    node.context.local_layout.insert(LocalLayout::PADDING);
                    style.set_padding(padding);
                }
                None => {
                    node.context.local_layout.remove(LocalLayout::PADDING);
                    style.set_padding(0.0);
                    let local = node.context.local_layout;
                    node.context
                        .control
                        .retheme(&theme, local, is_root, &mut style);
                }
            }
            aegle_layout::set_style(&mut self.tree, id, style)?;
        } else {
            node.context.padding = match padding {
                Some(padding) => {
                    let Length::Px(px) = padding.left else {
                        return Err(UiError::InvalidValue.into());
                    };
                    if Insets::all(px) != padding {
                        return Err(UiError::InvalidValue.into());
                    }
                    Some(px)
                }
                None => None,
            };
            self.tree.mark_dirty(id, Dirty::ALL)?;
        }
        Ok(())
    }

    /// Sets checked definite row and column gaps, or with `None` returns to
    /// the themed ones.
    pub fn set_gap(&mut self, id: NodeId, gap: Option<(Length, Length)>) -> Result {
        let is_root = id == self.root;
        let theme = *self.theme_of(id);
        let node = self.tree.get_mut(id).unwrap();
        let mut style = node.style().clone();
        match gap {
            Some((row, column)) => {
                node.context.local_layout.insert(LocalLayout::GAP);
                style.set_gap(row, column);
            }
            None => {
                node.context.local_layout.remove(LocalLayout::GAP);
                style.set_gap(0.0, 0.0);
                let local = node.context.local_layout;
                node.context
                    .control
                    .retheme(&theme, local, is_root, &mut style);
            }
        }
        aegle_layout::set_style(&mut self.tree, id, style)?;
        Ok(())
    }
}

impl Container {
    /// Sets the main axis and child order.
    pub fn set_direction(&self, direction: Direction) {
        self.layout(LocalLayout::NONE, |s| s.set_direction(direction))
    }
    /// Breaks children into several lines when they do not fit, like QML's `Flow`.
    pub fn set_wrap(&self, wrap: Wrap) {
        self.layout(LocalLayout::NONE, |s| s.set_wrap(wrap))
    }
    /// Aligns children on the cross axis (vertically in a row); `None`
    /// restores the default stretch. In a grid, the vertical alignment in each area.
    pub fn set_align_items(&self, align: impl Into<Option<Align>>) {
        let align = align.into();
        self.layout(LocalLayout::NONE, |s| s.set_align_items(align))
    }
    /// Distributes free main-axis space between children (horizontally in a
    /// row); in a grid, between columns. `None` packs at the start.
    pub fn set_justify_content(&self, justify: impl Into<Option<Justify>>) {
        let justify = justify.into();
        self.layout(LocalLayout::NONE, |s| s.set_justify_content(justify))
    }
    /// Distributes free cross-axis space between wrapped lines; in a grid,
    /// between rows. `None` restores the default stretch.
    pub fn set_align_content(&self, align: impl Into<Option<Justify>>) {
        let align = align.into();
        self.layout(LocalLayout::NONE, |s| s.set_align_content(align))
    }
}
