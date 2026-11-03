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
    /// Outer spacing: like padding, but may be negative or auto. On a layer
    /// window, whole dp distances from its anchored edges.
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
    /// Desktop-shell layer of a window document, which makes it a layer
    /// surface such as a panel: `background`, `bottom`, `top` or `overlay`.
    Layer,
    /// Output edges a layer attaches to: one of `top`, `bottom`, `left` and
    /// `right`, or a list of them.
    Anchor,
    /// Whole dp a layer reserves along its anchored edge, or `ignore` to
    /// overlap other surfaces' reserved zones.
    ExclusiveZone,
    /// A layer's keyboard focus policy: `none`, `exclusive` or `on_demand`.
    Keyboard,
    /// The name of the output a layer is shown on, such as `"DP-1"`.
    Output,
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
    /// Timing of every transitioned property: a duration, `[duration,
    /// easing]` or `spring(stiffness, damping)` over its settling time. An
    /// easing is `linear`, `ease_in`, `ease_out`, `ease_in_out`,
    /// `cubic_bezier(x1, y1, x2, y2)` or a spring; requires motion support.
    Transition,
    /// Paint transition timing, as [`Self::Transition`].
    PaintTransition,
    /// Offset transition timing, as [`Self::Transition`].
    OffsetTransition,
    /// Scale transition timing, as [`Self::Transition`].
    ScaleTransition,
    /// Rotation transition timing, as [`Self::Transition`].
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
    /// Shadow transition timing, as [`Self::Transition`].
    ShadowTransition,
    /// Opacity transition timing, as [`Self::Transition`].
    OpacityTransition,
    /// A scale animation started when the node is built, as
    /// [`Self::RotationAnimation`] with scales in `(0, 1000]`.
    ScaleAnimation,
    /// A rotation animation started when the node is built: `[values,
    /// duration]`, optionally followed by an easing as for
    /// [`Self::Transition`] and then `forever` or `alternate` (forever, every
    /// second cycle backwards). The two or more values, here in degrees, are
    /// spread evenly over the cycle; it plays once by default.
    RotationAnimation,
    /// An opacity animation started when the node is built, as
    /// [`Self::RotationAnimation`] with opacities in `[0, 1]`.
    OpacityAnimation,
    /// `[x, y, blur, spread, color]` inner shadow over the background, with
    /// a positive blur.
    InsetShadow,
    /// The corners `radius` rounds: one of `top_left`, `top_right`,
    /// `bottom_right` and `bottom_left`, or a list of distinct ones; every
    /// corner by default.
    Corners,
}

impl PropertyName {
    /// Transition timings and animations: a host installs them after every
    /// other property, and they need motion support.
    pub fn timed(self) -> bool {
        use PropertyName::*;
        matches!(
            self,
            Transition
                | PaintTransition
                | OffsetTransition
                | ScaleTransition
                | RotationTransition
                | ShadowTransition
                | OpacityTransition
                | ScaleAnimation
                | RotationAnimation
                | OpacityAnimation
        )
    }
}

pub(crate) const TOO_DEEP: &str = "nesting exceeds 256 levels";
