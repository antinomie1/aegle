//! Properties every node has, independent of any UI runtime: layout, style,
//! accessibility, motion and the document window's own properties.

mod constant;
mod grid;
mod values;

pub(crate) use constant::{constant, literal};
pub use values::choices;
pub(crate) use values::{Target, allowed, property_name, valid_id, validate};

/// Properties every node has, shared with the imperative retained-control
/// API; element properties come from each element's spec.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyName {
    /// Native window title.
    Title,
    /// Width: dp, percent or auto; a whole dp length on a window.
    Width,
    /// Height: dp, percent or auto; a whole dp length on a window.
    Height,
    /// Minimum width: dp, percent or auto.
    MinWidth,
    /// Minimum height: dp, percent or auto.
    MinHeight,
    /// Maximum width: dp, percent or auto (no limit).
    MaxWidth,
    /// Maximum height: dp, percent or auto (no limit).
    MaxHeight,
    /// Positive width-to-height ratio.
    AspectRatio,
    /// Inner spacing: one length or a CSS-order list of two or four.
    Padding,
    /// Outer spacing: like padding, but may be negative or auto.
    Margin,
    /// Absolute placement from the parent's edges, in CSS order; removes the item from the flow.
    Inset,
    /// Child spacing: one length, or `[row, column]` gaps.
    Gap,
    /// Nonnegative flex growth factor.
    Grow,
    /// Nonnegative flex shrink factor.
    Shrink,
    /// Main-axis size before growing and shrinking.
    Basis,
    /// Flex container main axis.
    Direction,
    /// Inline direction of the subtree: `ltr` or `rtl`; inherited when unset.
    LayoutDirection,
    /// Flex line wrapping.
    Wrap,
    /// Cross-axis alignment of children.
    Align,
    /// Distribution of main-axis free space.
    Justify,
    /// Distribution of space between lines or grid rows.
    AlignContent,
    /// This item's cross-axis alignment.
    AlignSelf,
    /// This grid item's horizontal alignment.
    JustifySelf,
    /// Grid children's horizontal alignment.
    JustifyItems,
    /// Explicit grid column tracks.
    Columns,
    /// Explicit grid row tracks.
    Rows,
    /// Implicit grid column tracks.
    AutoColumns,
    /// Implicit grid row tracks.
    AutoRows,
    /// Grid auto-placement order.
    Flow,
    /// Grid column placement: a line, or `[line or auto, span]`.
    GridColumn,
    /// Grid row placement: a line, or `[line or auto, span]`.
    GridRow,
    /// Named grid areas: one string of cell names per row.
    Areas,
    /// The grid area this child fills, by name.
    GridArea,
    /// A hint shown after the pointer rests, also the accessible description.
    Tooltip,
    /// Visibility of the subtree.
    Visible,
    /// Whether the subtree accepts interaction.
    Enabled,
    /// Explicit accessible name.
    Label,
    /// Named built-in window theme.
    Theme,
    /// Normal background color.
    Background,
    /// Normal text foreground color.
    Foreground,
    /// Outline color.
    BorderColor,
    /// Nonnegative outline thickness in logical pixels.
    BorderWidth,
    /// Nonnegative corner radius in logical pixels.
    Radius,
    /// Focus outline color for buttons, toggles, sliders and editors.
    FocusColor,
    /// Nonnegative focus outline thickness for focusable controls, in logical pixels.
    FocusWidth,
    /// Editor selection highlight color.
    SelectionColor,
    /// Editor caret color.
    CaretColor,
    /// Background color while hovered, for interactive controls.
    HoverBackground,
    /// Background color while a button, toggle or slider is pressed.
    PressedBackground,
    /// Background color while disabled.
    DisabledBackground,
    /// Text foreground color while disabled.
    DisabledForeground,
    /// Positive font size in logical pixels for text-bearing controls.
    FontSize,
    /// Timing of every transitioned property: a duration or `[duration,
    /// easing]`; requires motion support.
    Transition,
    /// Paint transition timing: a duration or `[duration, easing]`.
    PaintTransition,
    /// Offset transition timing: a duration or `[duration, easing]`.
    OffsetTransition,
    /// Scale transition timing: a duration or `[duration, easing]`.
    ScaleTransition,
    /// Rotation transition timing: a duration or `[duration, easing]`.
    RotationTransition,
    /// Horizontal presented translation in dp, after layout.
    OffsetX,
    /// Vertical presented translation in dp, after layout.
    OffsetY,
    /// Uniform presented scale about the center, in `(0, 1000]`.
    Scale,
    /// Clockwise presented rotation about the center, in degrees.
    Rotation,
    /// Check mark, switch thumb, slider thumb or progress fill color.
    IndicatorColor,
    /// `[x, y, blur, spread, color]` shadow beneath the node, or a shadow token.
    Shadow,
    /// `linear(degrees, stops...)` or `radial(stops...)` in place of the background color.
    BackgroundGradient,
    /// Opacity of the subtree composited as one group, in `[0, 1]`.
    Opacity,
    /// Nonnegative blur radius in dp of what lies behind the node.
    BackdropBlur,
    /// Shadow transition timing: a duration or `[duration, easing]`.
    ShadowTransition,
    /// Opacity transition timing: a duration or `[duration, easing]`.
    OpacityTransition,
}

pub(crate) const TOO_DEEP: &str = "nesting exceeds 256 levels";
