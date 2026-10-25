//! A node's layout style in Aegle's own vocabulary; the layout engine's
//! style stays inside this crate.

use taffy::{LengthPercentage, Position};

use crate::{Align, Direction, Insets, Justify, Length, Wrap};

/// How a node lays out its children, and whether it takes part in layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    /// A flex container, the default.
    #[default]
    Flex,
    /// Block layout: children stack vertically at full width.
    Block,
    /// A grid container (the `grid` feature).
    #[cfg(feature = "grid")]
    Grid,
    /// Hidden: no space and no layout for the subtree.
    None,
}

/// What happens to content larger than the node on one axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    /// Content overflows and enlarges scrollable ancestors.
    #[default]
    Visible,
    /// Content is clipped and contributes no overflow.
    Hidden,
    /// Content is clipped and scrolls; see [`crate::compute`] for how a
    /// scrolling axis measures.
    Scroll,
}

/// The inline direction of a node, read by flex, grid, block and leaf layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left: rows, grids and text start at the right edge.
    Rtl,
}

/// A node's layout style. Setters panic on a value outside the range they
/// document, such as a non-finite or (for sizes) negative length: styles are
/// built by code, so such a value is a programming error.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutStyle(pub(crate) taffy::Style);

fn check(valid: bool, what: &str) {
    assert!(valid, "invalid layout value: {what}");
}

fn size(length: Length, what: &str) -> Length {
    check(length.is_valid(true), what);
    length
}

impl LayoutStyle {
    /// A flex column, the default style.
    pub fn column() -> Self {
        let mut style = Self::default();
        style.set_direction(Direction::Column);
        style
    }

    /// A flex row.
    pub fn row() -> Self {
        let mut style = Self::default();
        style.set_direction(Direction::Row);
        style
    }

    /// Sets how the node lays out its children.
    pub fn set_display(&mut self, display: Display) -> &mut Self {
        self.0.display = match display {
            Display::Flex => taffy::Display::Flex,
            Display::Block => taffy::Display::Block,
            #[cfg(feature = "grid")]
            Display::Grid => taffy::Display::Grid,
            Display::None => taffy::Display::None,
        };
        self
    }

    /// How the node lays out its children.
    pub fn display(&self) -> Display {
        match self.0.display {
            taffy::Display::Block => Display::Block,
            #[cfg(feature = "grid")]
            taffy::Display::Grid => Display::Grid,
            taffy::Display::None => Display::None,
            _ => Display::Flex,
        }
    }

    /// Sets the width, finite and nonnegative.
    pub fn set_width(&mut self, width: impl Into<Length>) -> &mut Self {
        self.0.size.width = size(width.into(), "width").dimension();
        self
    }

    /// Sets the height, finite and nonnegative.
    pub fn set_height(&mut self, height: impl Into<Length>) -> &mut Self {
        self.0.size.height = size(height.into(), "height").dimension();
        self
    }

    /// Sets the minimum width; zero lets a flex item shrink below its content.
    pub fn set_min_width(&mut self, width: impl Into<Length>) -> &mut Self {
        self.0.min_size.width = size(width.into(), "minimum width").auto_length();
        self
    }

    /// Sets the minimum height.
    pub fn set_min_height(&mut self, height: impl Into<Length>) -> &mut Self {
        self.0.min_size.height = size(height.into(), "minimum height").auto_length();
        self
    }

    /// Sets the maximum width; automatic removes the limit.
    pub fn set_max_width(&mut self, width: impl Into<Length>) -> &mut Self {
        self.0.max_size.width = size(width.into(), "maximum width").auto_length();
        self
    }

    /// Sets the maximum height; automatic removes the limit.
    pub fn set_max_height(&mut self, height: impl Into<Length>) -> &mut Self {
        self.0.max_size.height = size(height.into(), "maximum height").auto_length();
        self
    }

    /// Keeps width divided by height at a positive ratio when one dimension
    /// is automatic; `None` removes the constraint.
    pub fn set_aspect_ratio(&mut self, ratio: Option<f32>) -> &mut Self {
        check(
            ratio.is_none_or(|r| r.is_finite() && r > 0.0),
            "aspect ratio",
        );
        self.0.aspect_ratio = ratio;
        self
    }

    /// Sets the main axis and child order of a flex container.
    pub fn set_direction(&mut self, direction: Direction) -> &mut Self {
        self.0.flex_direction = direction.flex();
        self
    }

    /// The main axis and child order of a flex container.
    pub fn direction(&self) -> Direction {
        match self.0.flex_direction {
            taffy::FlexDirection::Row => Direction::Row,
            taffy::FlexDirection::RowReverse => Direction::RowReverse,
            taffy::FlexDirection::ColumnReverse => Direction::ColumnReverse,
            taffy::FlexDirection::Column => Direction::Column,
        }
    }

    /// Sets whether a flex container breaks its children into lines.
    pub fn set_wrap(&mut self, wrap: Wrap) -> &mut Self {
        self.0.flex_wrap = wrap.flex();
        self
    }

    /// Sets a finite nonnegative flex grow factor.
    pub fn set_grow(&mut self, grow: f32) -> &mut Self {
        check(grow.is_finite() && grow >= 0.0, "grow factor");
        self.0.flex_grow = grow;
        self
    }

    /// Sets a finite nonnegative flex shrink factor; zero keeps the basis.
    pub fn set_shrink(&mut self, shrink: f32) -> &mut Self {
        check(shrink.is_finite() && shrink >= 0.0, "shrink factor");
        self.0.flex_shrink = shrink;
        self
    }

    /// Sets the main-axis size before growing and shrinking.
    pub fn set_basis(&mut self, basis: impl Into<Length>) -> &mut Self {
        self.0.flex_basis = size(basis.into(), "basis").dimension();
        self
    }

    /// Aligns children on the cross axis; `None` stretches them.
    pub fn set_align_items(&mut self, align: Option<Align>) -> &mut Self {
        self.0.align_items = align.map(Align::items);
        self
    }

    /// Overrides the parent's cross-axis alignment for this item.
    pub fn set_align_self(&mut self, align: Option<Align>) -> &mut Self {
        self.0.align_self = align.map(Align::items);
        self
    }

    /// Distributes free main-axis space between children; `None` packs them
    /// at the start.
    pub fn set_justify_content(&mut self, justify: Option<Justify>) -> &mut Self {
        self.0.justify_content = justify.map(Justify::content);
        self
    }

    /// Distributes free cross-axis space between wrapped lines or grid rows.
    pub fn set_align_content(&mut self, align: Option<Justify>) -> &mut Self {
        self.0.align_content = align.map(Justify::content);
        self
    }

    /// Sets outer spacing; edges may be negative or automatic.
    pub fn set_margin(&mut self, margin: impl Into<Insets>) -> &mut Self {
        let margin = margin.into();
        check(margin.is_valid(false, true), "margin");
        self.0.margin = margin.auto_lengths();
        self
    }

    /// Sets inner spacing: nonnegative, without automatic edges.
    pub fn set_padding(&mut self, padding: impl Into<Insets>) -> &mut Self {
        let padding = padding.into();
        check(padding.is_valid(true, false), "padding");
        self.0.padding = padding.definite();
        self
    }

    /// The inner spacing.
    pub fn padding(&self) -> Insets {
        let edge = |value: LengthPercentage| crate::values::from_definite(value);
        let padding = self.0.padding;
        Insets {
            left: edge(padding.left),
            top: edge(padding.top),
            right: edge(padding.right),
            bottom: edge(padding.bottom),
        }
    }

    /// Sets the spacing between rows and between columns, nonnegative and
    /// not automatic.
    pub fn set_gap(&mut self, row: impl Into<Length>, column: impl Into<Length>) -> &mut Self {
        let (row, column) = (row.into(), column.into());
        let gap = |length: Length| {
            check(length.is_valid(true), "gap");
            length
                .definite()
                .unwrap_or_else(|| panic!("invalid layout value: gap"))
        };
        self.0.gap = taffy::Size {
            width: gap(column),
            height: gap(row),
        };
        self
    }

    /// Takes the node out of its parent's flow and places it by `insets`
    /// from the parent's padding box; `None` returns it to the flow.
    pub fn set_absolute(&mut self, insets: Option<Insets>) -> &mut Self {
        match insets {
            Some(insets) => {
                check(insets.is_valid(false, true), "insets");
                self.0.position = Position::Absolute;
                self.0.inset = insets.auto_lengths();
            }
            None => {
                self.0.position = Position::Relative;
                self.0.inset = Insets::default().auto_lengths();
            }
        }
        self
    }

    /// Whether the node is placed out of its parent's flow.
    pub fn is_absolute(&self) -> bool {
        self.0.position == Position::Absolute
    }

    /// Sets what happens to content larger than the node, per axis.
    pub fn set_overflow(&mut self, x: Overflow, y: Overflow) -> &mut Self {
        let native = |overflow| match overflow {
            Overflow::Visible => taffy::Overflow::Visible,
            Overflow::Hidden => taffy::Overflow::Hidden,
            Overflow::Scroll => taffy::Overflow::Scroll,
        };
        self.0.overflow = taffy::Point {
            x: native(x),
            y: native(y),
        };
        self
    }

    /// What happens to content larger than the node, horizontally and vertically.
    pub fn overflow(&self) -> (Overflow, Overflow) {
        let own = |overflow| match overflow {
            taffy::Overflow::Hidden | taffy::Overflow::Clip => Overflow::Hidden,
            taffy::Overflow::Scroll => Overflow::Scroll,
            taffy::Overflow::Visible => Overflow::Visible,
        };
        (own(self.0.overflow.x), own(self.0.overflow.y))
    }

    /// Sets the inline direction.
    pub fn set_layout_direction(&mut self, direction: LayoutDirection) -> &mut Self {
        self.0.direction = match direction {
            LayoutDirection::Ltr => taffy::Direction::Ltr,
            LayoutDirection::Rtl => taffy::Direction::Rtl,
        };
        self
    }

    /// The inline direction.
    pub fn layout_direction(&self) -> LayoutDirection {
        match self.0.direction {
            taffy::Direction::Rtl => LayoutDirection::Rtl,
            _ => LayoutDirection::Ltr,
        }
    }
}
