//! Layout setters: an item's size, flex and placement on [`Node`], and how a
//! container arranges its children on [`Container`].
//!
//! Lengths accept logical pixels (`f32`), `Option<f32>` (`None` is automatic)
//! or [`Length`], including percentages of the parent's content box.

use aegle_core::{Dirty, NodeId};
use aegle_layout::{Align, Direction, Insets, Justify, Length, Position, Style, Wrap};
use aegle_theme::ControlKind;

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

fn check(valid: bool) -> Result {
    if valid {
        Ok(())
    } else {
        Err(UiError::InvalidValue.into())
    }
}

fn size(length: impl Into<Length>) -> Result<Length> {
    let length = length.into();
    check(length.is_valid(true))?;
    Ok(length)
}

impl Node {
    pub(crate) fn layout(&self, local: LocalLayout, change: impl FnOnce(&mut Style)) -> Result {
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
    /// Sets both dimensions; `None` or [`Length::Auto`] restores automatic sizing.
    pub fn set_size(&self, width: impl Into<Length>, height: impl Into<Length>) -> Result {
        let (width, height) = (size(width)?, size(height)?);
        self.layout(LocalLayout::HEIGHT, |s| {
            s.size.width = width.dimension();
            s.size.height = height.dimension();
        })
    }
    /// Sets the width, preserving height and its theme default.
    pub fn set_width(&self, width: impl Into<Length>) -> Result {
        let width = size(width)?;
        self.layout(LocalLayout::NONE, |s| s.size.width = width.dimension())
    }
    /// Sets the height; automatic replaces the control's themed height.
    pub fn set_height(&self, height: impl Into<Length>) -> Result {
        let height = size(height)?;
        self.layout(LocalLayout::HEIGHT, |s| s.size.height = height.dimension())
    }
    /// Sets both minimum dimensions; automatic is content-based for flex items.
    pub fn set_min_size(&self, width: impl Into<Length>, height: impl Into<Length>) -> Result {
        let (width, height) = (size(width)?, size(height)?);
        self.layout(LocalLayout::MIN_HEIGHT, |s| {
            s.min_size.width = width.auto_length();
            s.min_size.height = height.auto_length();
        })
    }
    /// Sets the minimum width without changing the minimum height.
    pub fn set_min_width(&self, width: impl Into<Length>) -> Result {
        let width = size(width)?;
        self.layout(LocalLayout::NONE, |s| {
            s.min_size.width = width.auto_length()
        })
    }
    /// Sets the minimum height, overriding the corresponding theme default.
    /// Zero lets a flex item shrink below its content, for example a column
    /// holding a scroll view or table.
    pub fn set_min_height(&self, height: impl Into<Length>) -> Result {
        let height = size(height)?;
        self.layout(LocalLayout::MIN_HEIGHT, |s| {
            s.min_size.height = height.auto_length()
        })
    }
    /// Sets both maximum dimensions; automatic removes the limit.
    pub fn set_max_size(&self, width: impl Into<Length>, height: impl Into<Length>) -> Result {
        let (width, height) = (size(width)?, size(height)?);
        self.layout(LocalLayout::NONE, |s| {
            s.max_size.width = width.auto_length();
            s.max_size.height = height.auto_length();
        })
    }
    /// Sets the maximum width; automatic removes the limit.
    pub fn set_max_width(&self, width: impl Into<Length>) -> Result {
        let width = size(width)?;
        self.layout(LocalLayout::NONE, |s| {
            s.max_size.width = width.auto_length()
        })
    }
    /// Sets the maximum height; automatic removes the limit.
    pub fn set_max_height(&self, height: impl Into<Length>) -> Result {
        let height = size(height)?;
        self.layout(LocalLayout::NONE, |s| {
            s.max_size.height = height.auto_length()
        })
    }
    /// Keeps width divided by height at a positive ratio when one dimension
    /// is automatic; `None` removes the constraint.
    pub fn set_aspect_ratio(&self, ratio: Option<f32>) -> Result {
        check(ratio.is_none_or(|r| r.is_finite() && r > 0.0))?;
        self.layout(LocalLayout::NONE, |s| s.aspect_ratio = ratio)
    }
    /// Sets a finite nonnegative flex grow factor; zero keeps intrinsic sizing.
    pub fn set_grow(&self, grow: f32) -> Result {
        check(grow.is_finite() && grow >= 0.0)?;
        self.layout(LocalLayout::NONE, |s| s.flex_grow = grow)
    }
    /// Sets how much this item gives up when its line overflows; zero keeps
    /// its basis. The default is one.
    pub fn set_shrink(&self, shrink: f32) -> Result {
        check(shrink.is_finite() && shrink >= 0.0)?;
        self.layout(LocalLayout::NONE, |s| s.flex_shrink = shrink)
    }
    /// Sets the main-axis size before growing and shrinking; automatic uses
    /// the size or content. Zero with a grow factor shares space by factor only.
    pub fn set_basis(&self, basis: impl Into<Length>) -> Result {
        let basis = size(basis)?;
        self.layout(LocalLayout::NONE, |s| s.flex_basis = basis.dimension())
    }
    /// Overrides the parent's cross-axis alignment for this item; in a grid,
    /// its vertical alignment within its area. `None` follows the parent.
    pub fn set_align_self(&self, align: Option<Align>) -> Result {
        self.layout(LocalLayout::NONE, |s| {
            s.align_self = align.map(Align::items)
        })
    }
    /// Sets outer spacing. Edges may be negative; `Auto` edges absorb free
    /// space, so automatic left and right margins center an item.
    pub fn set_margin(&self, margin: impl Into<Insets>) -> Result {
        let margin = margin.into();
        check(margin.is_valid(false, true))?;
        self.layout(LocalLayout::NONE, |s| s.margin = margin.auto_lengths())
    }
    /// Takes this item out of its parent's flow and places it by `insets`
    /// from the parent's padding box, over its siblings; `None` returns it to
    /// the flow. With opposite insets set and an automatic size, it stretches between them.
    pub fn set_absolute(&self, insets: Option<Insets>) -> Result {
        check(insets.is_none_or(|i| i.is_valid(false, true)))?;
        self.layout(LocalLayout::NONE, |s| match insets {
            Some(insets) => {
                s.position = Position::Absolute;
                s.inset = insets.auto_lengths();
            }
            None => {
                s.position = Position::Relative;
                s.inset = Insets::default().auto_lengths();
            }
        })
    }
    /// Sets inner spacing. Containers accept any [`Insets`]; text-bearing
    /// controls accept only one nonnegative pixel value on every edge.
    /// Ends a padding token binding.
    pub fn set_padding(&self, padding: impl Into<Insets>) -> Result {
        let padding = padding.into();
        check(padding.is_valid(true, false))?;
        self.change(|state, id| {
            state.set_padding(id, Some(padding))?;
            state.tokens.unbind(id, |s| s == LengthSlot::Padding.into());
            Ok(())
        })
    }
    /// Sets horizontal and vertical spacing between children.
    pub fn set_gap(&self, gap: impl Into<Length>) -> Result {
        let gap = gap.into();
        self.set_gaps(gap, gap)
    }
    /// Sets spacing between columns (horizontal) and between rows (vertical).
    /// Ends a gap token binding.
    pub fn set_gaps(&self, horizontal: impl Into<Length>, vertical: impl Into<Length>) -> Result {
        let (horizontal, vertical) = (horizontal.into(), vertical.into());
        check(horizontal.definite().is_some() && vertical.definite().is_some())?;
        check(horizontal.is_valid(true) && vertical.is_valid(true))?;
        self.change(|state, id| {
            state.set_gaps(id, Some((horizontal, vertical)))?;
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
        if matches!(
            node.context.control.kind(),
            ControlKind::Container | ControlKind::ScrollView
        ) {
            let mut style = node.style().clone();
            match padding {
                Some(padding) => {
                    node.context.local_layout.insert(LocalLayout::PADDING);
                    style.padding = padding.definite();
                }
                None => {
                    node.context.local_layout.remove(LocalLayout::PADDING);
                    style.padding = Insets::all(0.0).definite();
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
                    check(Insets::all(px) == padding)?;
                    Some(px)
                }
                None => None,
            };
            self.tree.mark_dirty(id, Dirty::ALL)?;
        }
        Ok(())
    }

    /// Sets checked definite gaps, or with `None` returns to the themed ones.
    pub fn set_gaps(&mut self, id: NodeId, gaps: Option<(Length, Length)>) -> Result {
        let is_root = id == self.root;
        let theme = *self.theme_of(id);
        let node = self.tree.get_mut(id).unwrap();
        let mut style = node.style().clone();
        match gaps {
            Some((horizontal, vertical)) => {
                node.context.local_layout.insert(LocalLayout::GAP);
                style.gap = aegle_layout::Size {
                    width: horizontal.definite().unwrap(),
                    height: vertical.definite().unwrap(),
                };
            }
            None => {
                node.context.local_layout.remove(LocalLayout::GAP);
                let zero = Length::Px(0.0).definite().unwrap();
                style.gap = aegle_layout::Size {
                    width: zero,
                    height: zero,
                };
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
    pub fn set_direction(&self, direction: Direction) -> Result {
        self.layout(LocalLayout::NONE, |s| s.flex_direction = direction.flex())
    }
    /// Breaks children into several lines when they do not fit, like QML's `Flow`.
    pub fn set_wrap(&self, wrap: Wrap) -> Result {
        self.layout(LocalLayout::NONE, |s| s.flex_wrap = wrap.flex())
    }
    /// Aligns children on the cross axis (vertically in a row); `None`
    /// restores the default stretch. In a grid, the vertical alignment in each area.
    pub fn set_align_items(&self, align: Option<Align>) -> Result {
        self.layout(LocalLayout::NONE, |s| {
            s.align_items = align.map(Align::items)
        })
    }
    /// Distributes free main-axis space between children (horizontally in a
    /// row); in a grid, between columns. `None` packs at the start.
    pub fn set_justify_content(&self, justify: Option<Justify>) -> Result {
        self.layout(LocalLayout::NONE, |s| {
            s.justify_content = justify.map(Justify::content)
        })
    }
    /// Distributes free cross-axis space between wrapped lines; in a grid,
    /// between rows. `None` restores the default stretch.
    pub fn set_align_content(&self, align: Option<Justify>) -> Result {
        self.layout(LocalLayout::NONE, |s| {
            s.align_content = align.map(Justify::content)
        })
    }
}
