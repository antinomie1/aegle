//! The controls inside sliders and progress bars: orientation, wheel steps,
//! eased value changes and indeterminate progress.

use std::time::Instant;

use aegle_controls::{Input, Outcome, Range};
use aegle_layout::{Dimension, Style};
use aegle_scene::{Affine, SceneBuilder, SceneError};
use aegle_text::TextSystem;
use aegle_theme::{Appearance, ControlKind, Theme};
use aegle_types::{Point, Size};
use aegle_ui::{
    Control, Result,
    control::{ControlVisual, Frame, InputCx, MeasureCx, PaintCx},
};

use crate::paint::{range, slider_track};

/// The axis a slider or progress bar runs along.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    /// Left to right, or right to left in a right-to-left layout.
    #[default]
    Horizontal,
    /// Bottom to top.
    Vertical,
}

/// Wheel distance in logical pixels for one slider step.
const WHEEL_STEP: f32 = 16.0;
/// Duration of an eased value change.
const GLIDE: f64 = 0.12;
/// One sweep of an indeterminate progress bar.
const SWEEP: f64 = 1.6;

/// The displayed fraction easing toward a new value, outside pointer drags.
#[derive(Default)]
pub(crate) struct Glide {
    from: f64,
    to: f64,
    start: Option<Instant>,
    moving: bool,
    shown: f64,
}

impl Glide {
    fn new(fraction: f64) -> Self {
        Self {
            from: fraction,
            to: fraction,
            shown: fraction,
            ..Self::default()
        }
    }
    /// Eases from the displayed fraction; `jump` shows the target at once.
    pub fn retarget(&mut self, to: f64, jump: bool) {
        if to == self.to {
            return;
        }
        self.from = self.shown;
        self.to = to;
        self.start = None;
        self.moving = !jump && cfg!(feature = "motion");
    }
    /// The fraction to draw at `now`, requesting another frame while easing.
    fn sample(&mut self, cx: &mut PaintCx<'_>) -> f64 {
        if !self.moving || cx.reduced_motion {
            self.moving = false;
            self.shown = self.to;
            return self.to;
        }
        let start = *self.start.get_or_insert(cx.time);
        let t = cx.time.duration_since(start).as_secs_f64() / GLIDE;
        if t >= 1.0 {
            self.moving = false;
            self.shown = self.to;
        } else {
            cx.request_frame();
            let eased = 1.0 - (1.0 - t).powi(3);
            self.shown = self.from + (self.to - self.from) * eased;
        }
        self.shown
    }
}

/// Paints a horizontal range layout, mirrored right to left, or rotated for a
/// vertical control so its track runs from bottom to top.
fn paint(
    builder: &mut SceneBuilder,
    size: Size,
    padding: f32,
    filled: [f64; 2],
    slider: bool,
    appearance: Appearance,
    [vertical, rtl]: [bool; 2],
) -> std::result::Result<(), SceneError> {
    if !vertical && rtl {
        builder.push_transform(Affine::new([-1.0, 0.0, 0.0, 1.0, size.width, 0.0])?)?;
        range(builder, size, padding, filled, slider, appearance)?;
        builder.pop()?;
        return Ok(());
    }
    if !vertical {
        return range(builder, size, padding, filled, slider, appearance);
    }
    // Local track coordinates (u along, v across) map to (v, height - u).
    builder.push_transform(Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, size.height])?)?;
    range(
        builder,
        Size::new(size.height, size.width),
        padding,
        filled,
        slider,
        appearance,
    )?;
    builder.pop()?;
    Ok(())
}

/// The fixed cross-axis extent of a themed control, in its orientation.
pub(crate) fn sized(style: &mut Style, extent: f32, vertical: bool) {
    let (fixed, free) = if vertical {
        (&mut style.size.width, &mut style.size.height)
    } else {
        (&mut style.size.height, &mut style.size.width)
    };
    *fixed = Dimension::length(extent);
    *free = Dimension::auto();
}

/// The control inside a [`crate::Slider`] node.
pub struct SliderControl {
    pub(crate) behavior: Box<aegle_controls::Slider>,
    pub(crate) vertical: bool,
    pub(crate) glide: Glide,
    focused: bool,
    wheel: f32,
}

impl SliderControl {
    pub(crate) fn new(range: Range) -> Self {
        let glide = Glide::new(range.fraction());
        Self {
            behavior: Box::new(aegle_controls::Slider::new(range)),
            vertical: false,
            glide,
            focused: false,
            wheel: 0.0,
        }
    }
    fn track(&self, size: Size, padding: f32) -> (f32, f32) {
        if self.vertical {
            slider_track(Size::new(size.height, size.width), padding)
        } else {
            slider_track(size, padding)
        }
    }
}

/// The control inside a [`crate::Progress`] node.
pub struct ProgressControl {
    pub(crate) range: Range,
    pub(crate) vertical: bool,
    pub(crate) indeterminate: bool,
    pub(crate) glide: Glide,
    /// The first frame painted while indeterminate: the sweep's origin.
    pub(crate) sweep: Option<Instant>,
}

impl ProgressControl {
    pub(crate) fn new(range: Range) -> Self {
        Self {
            glide: Glide::new(range.fraction()),
            range,
            vertical: false,
            indeterminate: false,
            sweep: None,
        }
    }
}

#[cfg(feature = "accessibility")]
fn numeric(node: &mut aegle_ui::accesskit::Node, range: &Range) {
    node.set_numeric_value(range.value());
    node.set_min_numeric_value(range.min());
    node.set_max_numeric_value(range.max());
}

#[cfg(feature = "accessibility")]
fn orientation(node: &mut aegle_ui::accesskit::Node, vertical: bool) {
    use aegle_ui::accesskit::Orientation;
    node.set_orientation(if vertical {
        Orientation::Vertical
    } else {
        Orientation::Horizontal
    });
}

fn measure(padding: f32, vertical: bool) -> Size {
    let (along, across) = (160.0, 20.0 + 2.0 * padding);
    if vertical {
        Size::new(across, along)
    } else {
        Size::new(along, across)
    }
}

impl Control for SliderControl {
    fn kind(&self) -> &'static ControlKind {
        &crate::kinds::SLIDER
    }
    fn interactive(&self) -> bool {
        true
    }
    fn drags(&self) -> bool {
        true
    }
    fn takes_wheel(&self) -> bool {
        self.focused
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn visual(&self) -> ControlVisual {
        ControlVisual {
            pressed: self.behavior.is_pressed(),
            hovered: Some(self.behavior.is_hovered()),
            ..Default::default()
        }
    }
    fn set_enabled(&mut self, _: &mut TextSystem, enabled: bool) -> Outcome {
        self.behavior.set_enabled(enabled)
    }
    fn content_offset(&self, size: Size, padding: f32, _: Point) -> Point {
        if self.vertical {
            Point::default()
        } else {
            Point::new(-slider_track(size, padding).0, 0.0)
        }
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let (start, extent) = self.track(cx.size, cx.padding);
        let input = match cx.logical(input) {
            // The behavior measures along x from the track start.
            Input::Pointer(mut pointer) if self.vertical => {
                let p = pointer.position;
                pointer.position = Point::new(cx.size.height - p.y - start, p.x);
                Input::Pointer(pointer)
            }
            // Right to left the track starts at the right end.
            Input::Pointer(mut pointer) if cx.rtl => {
                pointer.position.x = extent - pointer.position.x;
                Input::Pointer(pointer)
            }
            Input::Focus(focused) => {
                self.focused = focused;
                self.wheel = 0.0;
                input
            }
            Input::Wheel { delta, .. } => {
                let across = if cx.rtl { -delta.x } else { delta.x };
                self.wheel += across - delta.y;
                let mut outcome = Outcome {
                    handled: true,
                    ..Outcome::default()
                };
                while self.wheel.abs() >= WHEEL_STEP {
                    let step = if self.wheel > 0.0 {
                        Input::Increment
                    } else {
                        Input::Decrement
                    };
                    self.wheel -= WHEEL_STEP.copysign(self.wheel);
                    let changed = self.behavior.handle(step, extent)?;
                    outcome.repaint |= changed.repaint;
                    outcome.semantics |= changed.semantics;
                    outcome.action = outcome.action.or(changed.action);
                }
                self.glide.retarget(self.behavior.range().fraction(), false);
                return Ok(outcome);
            }
            input => input,
        };
        let outcome = self.behavior.handle(input, extent)?;
        self.glide
            .retarget(self.behavior.range().fraction(), self.behavior.is_pressed());
        Ok(outcome)
    }
    fn hover(
        &mut self,
        _: &mut InputCx<'_>,
        pointer: aegle_ui::PointerId,
        _: Input<'_>,
    ) -> Result<Outcome> {
        Ok(self.behavior.update_hover(pointer, true))
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        Ok(measure(cx.padding, self.vertical))
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, _: bool, style: &mut Style) {
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            sized(style, theme.control_height, self.vertical);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        let shown = self.glide.sample(cx);
        paint(
            cx.builder,
            cx.size,
            cx.padding,
            [0.0, shown],
            true,
            *cx.appearance,
            [self.vertical, cx.rtl],
        )?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Action, Role};
        cx.node.set_role(Role::Slider);
        orientation(cx.node, self.vertical);
        let range = self.behavior.range();
        numeric(cx.node, range);
        cx.node.set_numeric_value_step(if range.step() == 0.0 {
            (range.max() - range.min()) / 100.0
        } else {
            range.step()
        });
        if cx.enabled {
            cx.node.add_action(Action::Focus);
            cx.node.add_action(Action::SetValue);
            cx.node.add_action(Action::Increment);
            cx.node.add_action(Action::Decrement);
        }
    }
    #[cfg(feature = "accessibility")]
    fn action_input(
        &self,
        action: aegle_ui::accesskit::Action,
        data: Option<&aegle_ui::accesskit::ActionData>,
    ) -> Option<Input<'static>> {
        use aegle_ui::accesskit::{Action, ActionData};
        match (action, data) {
            (Action::Increment, _) => Some(Input::Increment),
            (Action::Decrement, _) => Some(Input::Decrement),
            (Action::SetValue, Some(ActionData::NumericValue(value))) if value.is_finite() => {
                Some(Input::SetValue(*value))
            }
            _ => None,
        }
    }
}

impl Control for ProgressControl {
    fn kind(&self) -> &'static ControlKind {
        &crate::kinds::PROGRESS
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        Ok(measure(cx.padding, self.vertical))
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, _: bool, style: &mut Style) {
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            sized(style, theme.control_height / 2.0, self.vertical);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        let filled = if !self.indeterminate {
            [0.0, self.glide.sample(cx)]
        } else if cx.reduced_motion {
            // A still, centered segment: busy without motion.
            [0.3, 0.7]
        } else {
            cx.request_frame();
            let epoch = *self.sweep.get_or_insert(cx.time);
            let phase = (cx.time.duration_since(epoch).as_secs_f64() % SWEEP) / SWEEP;
            let head = phase * 1.3;
            [head - 0.3, head]
        };
        paint(
            cx.builder,
            cx.size,
            cx.padding,
            filled,
            false,
            *cx.appearance,
            [self.vertical, cx.rtl],
        )?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.node
            .set_role(aegle_ui::accesskit::Role::ProgressIndicator);
        orientation(cx.node, self.vertical);
        // An indeterminate indicator reports no value.
        if !self.indeterminate {
            numeric(cx.node, &self.range);
        }
    }
}
