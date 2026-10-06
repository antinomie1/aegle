//! A small, validated vocabulary over the Taffy style fields Aegle uses.
//!
//! Hosts accept these values at their public boundary, check them with the
//! `is_valid` methods once, and convert them into [`taffy::Style`] fields. Raw Taffy
//! types stay available for hosts that need the full style.

use taffy::{
    AlignContent, AlignItems, Dimension, FlexDirection, FlexWrap, LengthPercentage,
    LengthPercentageAuto,
};

/// A logical length: pixels, a percentage of the parent's content box, or automatic.
///
/// Percentages use 0–100. A percentage of an indefinite parent size behaves
/// like `Auto`. Lengths are logical pixels before the window scale.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Length {
    /// Sized from content and the parent's layout.
    #[default]
    Auto,
    /// Logical pixels.
    Px(f32),
    /// Percent of the parent's content box on the same axis (inline axis for margins and padding).
    Percent(f32),
}

impl Length {
    /// Whether the value is finite and, if `nonnegative`, not negative.
    pub fn is_valid(self, nonnegative: bool) -> bool {
        match self {
            Self::Auto => true,
            Self::Px(v) | Self::Percent(v) => v.is_finite() && (!nonnegative || v >= 0.0),
        }
    }

    /// As a size, basis or other dimension.
    pub fn dimension(self) -> Dimension {
        match self {
            Self::Auto => Dimension::auto(),
            Self::Px(v) => Dimension::length(v),
            Self::Percent(v) => Dimension::percent(v / 100.0),
        }
    }

    /// As a minimum/maximum size, margin or inset.
    pub fn auto_length(self) -> LengthPercentageAuto {
        match self {
            Self::Auto => LengthPercentageAuto::auto(),
            Self::Px(v) => LengthPercentageAuto::length(v),
            Self::Percent(v) => LengthPercentageAuto::percent(v / 100.0),
        }
    }

    /// As padding or gap, which have no automatic value: `None` for `Auto`.
    pub fn definite(self) -> Option<LengthPercentage> {
        match self {
            Self::Auto => None,
            Self::Px(v) => Some(LengthPercentage::length(v)),
            Self::Percent(v) => Some(LengthPercentage::percent(v / 100.0)),
        }
    }
}

impl From<f32> for Length {
    fn from(px: f32) -> Self {
        Self::Px(px)
    }
}

impl From<Option<f32>> for Length {
    fn from(px: Option<f32>) -> Self {
        px.map_or(Self::Auto, Self::Px)
    }
}

/// Lengths for the four edges of a box: margins, padding or insets.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Left edge.
    pub left: Length,
    /// Top edge.
    pub top: Length,
    /// Right edge.
    pub right: Length,
    /// Bottom edge.
    pub bottom: Length,
}

impl Insets {
    /// The same length on every edge.
    pub fn all(length: impl Into<Length>) -> Self {
        let length = length.into();
        Self::symmetric(length, length)
    }

    /// `horizontal` on the left and right, `vertical` on the top and bottom.
    pub fn symmetric(horizontal: impl Into<Length>, vertical: impl Into<Length>) -> Self {
        let (horizontal, vertical) = (horizontal.into(), vertical.into());
        Self {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        }
    }

    /// Edges in CSS order: top, right, bottom, left.
    pub fn new(
        top: impl Into<Length>,
        right: impl Into<Length>,
        bottom: impl Into<Length>,
        left: impl Into<Length>,
    ) -> Self {
        Self {
            left: left.into(),
            top: top.into(),
            right: right.into(),
            bottom: bottom.into(),
        }
    }

    fn edges(self) -> [Length; 4] {
        [self.left, self.top, self.right, self.bottom]
    }

    /// Whether every edge is valid; padding additionally forbids `Auto`.
    pub fn is_valid(self, nonnegative: bool, allow_auto: bool) -> bool {
        self.edges()
            .iter()
            .all(|edge| edge.is_valid(nonnegative) && (allow_auto || *edge != Length::Auto))
    }

    /// As margins or insets.
    pub fn auto_lengths(self) -> taffy::Rect<LengthPercentageAuto> {
        taffy::Rect {
            left: self.left.auto_length(),
            right: self.right.auto_length(),
            top: self.top.auto_length(),
            bottom: self.bottom.auto_length(),
        }
    }

    /// As padding; `Auto` edges (rejected by [`Self::is_valid`]) become zero.
    pub fn definite(self) -> taffy::Rect<LengthPercentage> {
        let edge = |length: Length| length.definite().unwrap_or(LengthPercentage::length(0.0));
        taffy::Rect {
            left: edge(self.left),
            right: edge(self.right),
            top: edge(self.top),
            bottom: edge(self.bottom),
        }
    }
}

impl From<f32> for Insets {
    fn from(px: f32) -> Self {
        Self::all(px)
    }
}

impl From<Length> for Insets {
    fn from(length: Length) -> Self {
        Self::all(length)
    }
}

/// Placement of children on the cross axis, or of one item in its line or grid area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// The start edge.
    Start,
    /// The end edge.
    End,
    /// Centered.
    Center,
    /// Filling the line or area unless the item has an explicit size.
    Stretch,
    /// Sharing the first text baseline of the line.
    Baseline,
}

impl Align {
    /// As Taffy's item alignment.
    pub fn items(self) -> AlignItems {
        match self {
            Self::Start => AlignItems::START,
            Self::End => AlignItems::END,
            Self::Center => AlignItems::CENTER,
            Self::Stretch => AlignItems::STRETCH,
            Self::Baseline => AlignItems::BASELINE,
        }
    }
}

/// Distribution of free space between children or lines on an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    /// Packed at the start.
    Start,
    /// Packed at the end.
    End,
    /// Packed in the middle.
    Center,
    /// Lines or grid tracks grow to fill; on the flex main axis, like `Start`.
    Stretch,
    /// First and last at the edges, equal space between.
    SpaceBetween,
    /// Equal space around each, half-size at the edges.
    SpaceAround,
    /// Equal space between and at the edges.
    SpaceEvenly,
}

impl Justify {
    /// As Taffy's content alignment.
    pub fn content(self) -> AlignContent {
        match self {
            Self::Start => AlignContent::START,
            Self::End => AlignContent::END,
            Self::Center => AlignContent::CENTER,
            Self::Stretch => AlignContent::STRETCH,
            Self::SpaceBetween => AlignContent::SPACE_BETWEEN,
            Self::SpaceAround => AlignContent::SPACE_AROUND,
            Self::SpaceEvenly => AlignContent::SPACE_EVENLY,
        }
    }
}

/// The main axis of a flex container and the order of its children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Left to right.
    Row,
    /// Top to bottom.
    Column,
    /// Right to left.
    RowReverse,
    /// Bottom to top.
    ColumnReverse,
}

impl Direction {
    /// As Taffy's flex direction.
    pub fn flex(self) -> FlexDirection {
        match self {
            Self::Row => FlexDirection::Row,
            Self::Column => FlexDirection::Column,
            Self::RowReverse => FlexDirection::RowReverse,
            Self::ColumnReverse => FlexDirection::ColumnReverse,
        }
    }
}

/// Whether a flex container breaks its children into several lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wrap {
    /// One line; children shrink or overflow.
    #[default]
    NoWrap,
    /// New lines after the first, in the cross direction.
    Wrap,
    /// New lines before the first.
    WrapReverse,
}

impl Wrap {
    /// As Taffy's flex wrap.
    pub fn flex(self) -> FlexWrap {
        match self {
            Self::NoWrap => FlexWrap::NoWrap,
            Self::Wrap => FlexWrap::Wrap,
            Self::WrapReverse => FlexWrap::WrapReverse,
        }
    }
}
