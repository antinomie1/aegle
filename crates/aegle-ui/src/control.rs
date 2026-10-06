//! The seam between the retained UI engine and its controls.
//!
//! The engine owns the tree, layout, input routing, focus, scrolling, motion,
//! themes and semantics export, and knows nothing about particular controls.
//! A control is a [`Control`] stored in each node; the engine asks it only the
//! questions below and otherwise treats it as opaque. Controls live in
//! `aegle-widgets`, and any crate can add its own by implementing this trait.
//! Three capabilities are first-class because the engine's own machinery needs
//! them: a [`Paragraph`] (font size, themes, labels), a [`TextField`] editor
//! (native IME, caret scrolling, clipboard) and a scroll viewport.

use std::{any::Any, cell::RefCell};

use aegle_controls::{Input, Outcome, TextField};
use aegle_core::NodeId;
use aegle_layout::Style;
use aegle_scene::{Color, RoundedRect, SceneBuilder};
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{Appearance, ControlKind, Theme, VisualState};
use aegle_types::{Point, Size};

use crate::{Result, bar::Bar, state::State};

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

/// Which local style fields a control accepts; others are rejected as `WrongKind`.
#[derive(Clone, Copy, Debug, Default)]
pub struct StyleScope {
    /// Hover/pressed backgrounds and focus colors (buttons, toggles, sliders).
    pub button_like: bool,
    /// Selection and caret colors, and hover/focus (editors).
    pub editor: bool,
    /// Indicator color (toggle marks, slider and progress fills).
    pub indicator: bool,
}

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
}

/// Intrinsic size measurement context.
pub struct MeasureCx<'a> {
    /// Shared fonts.
    pub fonts: &'a RefCell<TextSystem>,
    /// The control's content padding.
    pub padding: f32,
    /// The theme gap between a control's own parts.
    pub gap: f32,
    /// The width the layout offers, if it is definite.
    pub width: Option<f32>,
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
    /// Upcast for typed handles.
    fn as_any(&self) -> &dyn Any;
    /// Mutable upcast for typed handles.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// The role a skin styles this control as.
    fn kind(&self) -> ControlKind;
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
    /// Local style fields it accepts.
    fn style_scope(&self) -> StyleScope {
        StyleScope::default()
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
    /// Re-lays out retained text for the final width after layout.
    fn finalize(&mut self, _fonts: &RefCell<TextSystem>, _width: f32, _padding: f32) -> Result {
        Ok(())
    }
    /// Adjusts the layout style that follows the theme when it changes, leaving
    /// fields in `local` (bits the application set) alone.
    fn retheme(&self, _theme: &Theme, _local: u8, _root: bool, _style: &mut Style) {}

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
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Container
    }
    fn retheme(&self, theme: &Theme, local: u8, root: bool, style: &mut Style) {
        use aegle_layout::{Edges, LengthPercentage, Size as LayoutSize};
        if local & 4 == 0 {
            let gap = LengthPercentage::length(theme.gap);
            style.gap = LayoutSize {
                width: gap,
                height: gap,
            };
        }
        if root && local & 2 == 0 {
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
