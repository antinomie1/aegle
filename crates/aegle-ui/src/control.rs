//! The seam between the retained UI engine and its controls.
//!
//! The engine owns the tree, layout, input routing, focus, scrolling, motion,
//! themes and semantics export, and knows nothing about particular controls.
//! A control is a [`Control`] stored in each node; the engine asks it only the
//! questions below and otherwise treats it as opaque. Controls live in
//! `aegle-widgets`, and any crate can add its own by implementing this trait.
//! Three capabilities are first-class because the engine's own machinery needs
//! them: a [`Paragraph`] (font size, themes, labels), a [`TextField`] editor
//! (native IME, caret scrolling, clipboard) and a scroll viewport. The input
//! and outcome types a control handles are re-exported here, so a control
//! library needs no direct `aegle-controls` dependency.

use std::{any::Any, cell::RefCell};

use aegle_controls::TextField;
pub use aegle_controls::{Action, Capture, Input, Outcome, PointerInput};
use aegle_core::NodeId;
use aegle_layout::Style;
use aegle_scene::{Color, RoundedRect, SceneBuilder};
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{Accepts, Appearance, ControlKind, Theme, VisualState};
use aegle_types::{Point, Size};

use crate::{LocalLayout, Result, bar::Bar, state::State};

/// Work a control asks the engine to run after its input is handled, with the
/// node that handled it. It runs before the outcome's effects, so it can change
/// other controls (for example unchecking sibling radio buttons).
pub type Deferred = Box<dyn FnOnce(&mut State, NodeId) -> Result>;

/// Interaction state a control adds to the [`VisualState`] its skin receives.
#[derive(Clone, Copy, Debug, Default)]
pub struct ControlVisual {
    /// A pointer or key holds the control down.
    pub pressed: bool,
    /// The control tracks its own hover; `None` uses the engine's hover node.
    pub hovered: Option<bool>,
    /// An editor that allows selection but not edits.
    pub read_only: bool,
    /// A toggle in its checked state.
    pub checked: bool,
}

/// The kind of plain rows, columns and other layout-only containers:
/// transparent, accepting no kind-specific style.
pub static CONTAINER: ControlKind = ControlKind {
    name: "Container",
    skin: Appearance::base,
    accepts: Accepts::NONE,
    container: true,
};

/// Whether the engine paints the common background and border before the control.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Fill the appearance background.
    pub background: bool,
    /// Stroke the appearance border.
    pub border: bool,
}

/// Input handling context.
pub struct InputCx<'a> {
    /// Shared fonts, for editors.
    pub fonts: &'a mut TextSystem,
    /// The control's last laid out size.
    pub size: Size,
    /// The control's content padding.
    pub padding: f32,
    /// When the platform reported this input; see [`crate::Ui::key_at`].
    pub time: std::time::Instant,
    /// Work to run after this call, see [`Deferred`].
    pub deferred: &'a mut Vec<Deferred>,
    /// The control is laid out right to left, see [`InputCx::logical`].
    pub rtl: bool,
}

impl InputCx<'_> {
    /// `input` with Left and Right arrow keys exchanged right to left, for
    /// controls whose arrow keys move toward a logical start or end (sliders,
    /// tabs, splitters) rather than a visual side (text carets).
    pub fn logical<'i>(&self, input: Input<'i>) -> Input<'i> {
        match input {
            Input::Key(mut key) if self.rtl => {
                key.key = match key.key {
                    aegle_controls::Key::Left => aegle_controls::Key::Right,
                    aegle_controls::Key::Right => aegle_controls::Key::Left,
                    other => other,
                };
                Input::Key(key)
            }
            input => input,
        }
    }
}

/// Intrinsic size measurement context.
pub struct MeasureCx<'a> {
    /// Shared fonts.
    pub fonts: &'a RefCell<TextSystem>,
    /// The control's content padding.
    pub padding: f32,
    /// The theme gap between a control's own parts.
    pub gap: f32,
    /// The width the layout offers, if it is definite; always the final width
    /// in [`Control::finalize`].
    pub width: Option<f32>,
    /// The control is laid out right to left: its text aligns right.
    pub rtl: bool,
}

impl MeasureCx<'_> {
    /// Paragraph alignment for the layout direction: the left edge, or the right
    /// one right to left, whatever direction the text itself runs in.
    pub fn alignment(&self) -> aegle_text::Alignment {
        if self.rtl {
            aegle_text::Alignment::Right
        } else {
            aegle_text::Alignment::Left
        }
    }
    /// The width offered to content inside the padding.
    pub fn content_width(&self) -> Option<f32> {
        self.width.map(|w| (w - 2.0 * self.padding).max(0.0))
    }
}

/// Drawing context; the scene is in the control's local coordinates.
pub struct PaintCx<'a> {
    /// Records commands.
    pub builder: &'a mut SceneBuilder,
    /// Laid out size.
    pub size: Size,
    /// Content padding.
    pub padding: f32,
    /// The node's resolved theme.
    pub theme: &'a Theme,
    /// Resolved (and presented, with motion) appearance.
    pub appearance: &'a Appearance,
    /// Effective interaction state.
    pub visual: VisualState,
    /// Internal scroll offset of an editor.
    pub scroll: Point,
    /// The rounded bounds shape.
    pub shape: RoundedRect,
    /// Vertical then horizontal scrollbar geometry, for editors and viewports.
    pub bars: [Option<Bar>; 2],
    /// Track and thumb colors.
    pub bar_color: [Color; 2],
    /// The current frame's time, see [`crate::Ui::run_frame`].
    pub time: std::time::Instant,
    /// Whether the window prefers reduced motion; continuous decoration should stop.
    pub reduced_motion: bool,
    /// The control is laid out right to left: directional parts are mirrored.
    pub rtl: bool,
    /// Set by [`PaintCx::request_frame`].
    pub(crate) next_frame: bool,
}

impl PaintCx<'_> {
    /// Repaints this control on the next frame, for animated content such as an
    /// indeterminate progress bar. Ask again on every paint that still moves.
    pub fn request_frame(&mut self) {
        self.next_frame = true;
    }
}

/// Semantic export context for controls without an editor or viewport.
#[cfg(feature = "accessibility")]
pub struct SemanticsCx<'a> {
    /// The node to describe; its bounds and transform are already set.
    pub node: &'a mut aegle_access::accesskit::Node,
    /// Whether the control can currently be used.
    pub enabled: bool,
    /// Whether the application set an explicit accessible label.
    pub labelled: bool,
}

/// One control inside a node. See the [module docs](self).
pub trait Control: Any {
    /// The control's kind: its default skin and the local style it accepts.
    /// A kind accepting [`Accepts::EDITOR`] has an [`Control::editor`] and no
    /// other kind does.
    fn kind(&self) -> &'static ControlKind;
    /// Takes keyboard focus and input.
    fn interactive(&self) -> bool {
        false
    }
    /// A pointer drag that starts here belongs to the control, not to panning.
    fn drags(&self) -> bool {
        false
    }
    /// Receives wheel input as [`Input::Wheel`] before enclosing scroll
    /// views; a handled outcome consumes the scroll.
    fn takes_wheel(&self) -> bool {
        false
    }
    /// A clipped, scrollable viewport for its children. It also receives an
    /// overlay record drawn after them, see [`Control::paint_overlay`].
    fn viewport(&self) -> bool {
        false
    }
    /// Clips its own contents, so its intrinsic overflow never enlarges an
    /// ancestor's scrollable area.
    fn self_clipping(&self) -> bool {
        false
    }
    /// Common background and border painting.
    fn frame(&self) -> Frame {
        Frame {
            background: true,
            border: true,
        }
    }
    /// Its intrinsic size depends on the theme gap.
    fn uses_gap(&self) -> bool {
        false
    }
    /// Content padding unless the application set one.
    fn default_padding(&self, theme: &Theme) -> f32 {
        theme.padding
    }

    /// The display paragraph of a text-bearing control.
    fn paragraph(&self) -> Option<&Paragraph> {
        None
    }
    /// Mutable [`Control::paragraph`].
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        None
    }
    /// The editor of an editable control.
    fn editor(&self) -> Option<&TextField> {
        None
    }
    /// Mutable [`Control::editor`].
    fn editor_mut(&mut self) -> Option<&mut TextField> {
        None
    }

    /// Interaction state for skins.
    fn visual(&self) -> ControlVisual {
        ControlVisual::default()
    }
    /// Reacts to the control becoming usable or unusable.
    fn set_enabled(&mut self, _fonts: &mut TextSystem, _enabled: bool) -> Outcome {
        Outcome::default()
    }
    /// Where its content starts, added to pointer positions given to the control:
    /// an editor's scrolled text origin, a slider's track start.
    fn content_offset(&self, _size: Size, _padding: f32, _scroll: Point) -> Point {
        Point::default()
    }
    /// The pointer moved over the control while no other control holds it.
    fn hover(
        &mut self,
        _cx: &mut InputCx<'_>,
        _pointer: aegle_controls::PointerId,
        _input: Input<'_>,
    ) -> Result<Outcome> {
        Ok(Outcome::default())
    }
    /// Handles one routed input event.
    fn handle(&mut self, _cx: &mut InputCx<'_>, _input: Input<'_>) -> Result<Outcome> {
        Ok(Outcome::default())
    }

    /// Intrinsic size including padding; containers measure zero.
    fn measure(&mut self, _cx: &MeasureCx<'_>) -> Result<Size> {
        Ok(Size::default())
    }
    /// The first text baseline from the top of a box of `size` laid out by the
    /// last measurement, for baseline alignment; `None` aligns the bottom edge.
    fn baseline(&self, _size: Size, _padding: f32) -> Option<f32> {
        None
    }
    /// Re-lays out retained text for the final width after layout, given as
    /// `cx.width`.
    fn finalize(&mut self, _cx: &MeasureCx<'_>) -> Result {
        Ok(())
    }
    /// Adjusts the layout style that follows the theme when it changes, leaving
    /// fields in `local` (bits the application set) alone.
    fn retheme(&self, _theme: &Theme, _local: LocalLayout, _root: bool, _style: &mut Style) {}

    /// Records the control's content.
    fn paint(&mut self, _cx: &mut PaintCx<'_>) -> Result {
        Ok(())
    }
    /// Records what a viewport draws over its children: border and scrollbars.
    fn paint_overlay(&mut self, _cx: &mut PaintCx<'_>) -> Result {
        Ok(())
    }
    /// The input a semantic action (click, increment, set value) stands for, if
    /// this control supports it.
    #[cfg(feature = "accessibility")]
    fn action_input(
        &self,
        _action: aegle_access::accesskit::Action,
        _data: Option<&aegle_access::accesskit::ActionData>,
    ) -> Option<Input<'static>> {
        None
    }
    /// Describes the control to assistive technology.
    #[cfg(feature = "accessibility")]
    fn semantics(&self, _cx: &mut SemanticsCx<'_>) {}
}

/// A plain row or column: layout only.
pub struct Plain;

impl Control for Plain {
    fn kind(&self) -> &'static ControlKind {
        &CONTAINER
    }
    fn retheme(&self, theme: &Theme, local: LocalLayout, root: bool, style: &mut Style) {
        use aegle_layout::{Edges, LengthPercentage, Size as LayoutSize};
        if !local.contains(LocalLayout::GAP) {
            let gap = LengthPercentage::length(theme.gap);
            style.gap = LayoutSize {
                width: gap,
                height: gap,
            };
        }
        if root && !local.contains(LocalLayout::PADDING) {
            let p = LengthPercentage::length(theme.padding);
            style.padding = Edges {
                left: p,
                right: p,
                top: p,
                bottom: p,
            };
        }
    }
}
