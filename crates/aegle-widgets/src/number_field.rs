//! A single-line numeric editor with steppers, arrow keys and wheel steps.

use aegle_controls::{Action, Input, Key, Outcome, PointerKind, Range};
use aegle_core::Dirty;
use aegle_layout::Style;
use aegle_scene::{Affine, PathBuilder, Point as ScenePoint, Rect, RoundedRect};
use aegle_text::{Selection, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::{Point, Size};
use aegle_ui::{
    Container, Control, Result, UiError,
    control::{ControlVisual, InputCx, MeasureCx, PaintCx},
    handle,
};

use crate::field::FieldControl;

handle! {
    /// A numeric text field: typing, steppers, Up/Down/PageUp/PageDown and the wheel (while focused) change one clamped value.
    pub NumberField(NumberFieldControl): text, interactive, editor
}

/// Width of the stepper strip at the field's end edge (left right to left).
const STRIP: f32 = 20.0;
/// Wheel distance in logical pixels for one step.
const WHEEL_STEP: f32 = 16.0;

/// The control inside a [`NumberField`] node.
pub struct NumberFieldControl {
    field: FieldControl,
    range: Range,
    decimals: u8,
    focused: bool,
    wheel: f32,
    /// Last painted editor scroll, to map pointer positions back to the field.
    scroll: Point,
    /// Laid out right to left, with the steppers on the left.
    rtl: bool,
}

impl NumberFieldControl {
    /// Where the text area starts: after a leading stepper strip right to left.
    fn lead(&self) -> f32 {
        if self.rtl { STRIP } else { 0.0 }
    }
    fn format(&self) -> String {
        format!("{:.*}", usize::from(self.decimals), self.range.value())
    }
    /// The step for arrows, steppers and the wheel: the range step, or 1% of the span.
    fn step(&self) -> f64 {
        match self.range.step() {
            0.0 => (self.range.max() - self.range.min()) / 100.0,
            step => step,
        }
    }
    /// Shows the value as text, selecting it while focused for quick retyping.
    fn show(&mut self, fonts: &mut TextSystem) -> Result {
        let text = self.format();
        let mut editor = fonts.edit(self.field.0.editor_mut());
        editor.set_text(&text)?;
        if self.focused {
            editor.select(Selection {
                anchor: 0,
                focus: text.len(),
            })?;
        }
        Ok(())
    }
    /// Sets the value and reports a change through the action handler.
    fn assign(&mut self, fonts: &mut TextSystem, value: f64) -> Result<Outcome> {
        let changed = self.range.set_value(value)?;
        self.show(fonts)?;
        Ok(Outcome {
            handled: true,
            repaint: true,
            semantics: true,
            reset_ime: true,
            action: changed.then_some(Action::Change),
            ..Outcome::default()
        })
    }
    /// Parses typed text; invalid text restores the current value.
    fn commit(&mut self, fonts: &mut TextSystem) -> Result<Outcome> {
        let typed = self.field.0.editor().text().to_string();
        match typed.trim().parse::<f64>() {
            Ok(value) if value.is_finite() => self.assign(fonts, value),
            _ => self.assign(fonts, self.range.value()),
        }
    }
}

impl NumberField {
    /// Returns the committed value; text being typed counts after Enter or blur.
    pub fn value(&self) -> Result<f64> {
        self.read(|control| control.range.value())
    }
    /// Sets a finite value, clamped to the range, without a change callback.
    pub fn set_value(&self, value: f64) -> Result {
        self.edit(|control, fonts| {
            control.range.set_value(value)?;
            control.show(fonts)
        })
    }
    /// Returns the inclusive bounds.
    pub fn range(&self) -> Result<(f64, f64)> {
        self.read(|control| (control.range.min(), control.range.max()))
    }
    /// Replaces finite increasing bounds, clamping the value.
    pub fn set_range(&self, min: f64, max: f64) -> Result {
        self.edit(|control, fonts| {
            control.range.set_bounds(min, max)?;
            control.show(fonts)
        })
    }
    /// Sets a finite nonnegative step; zero steps by 1% of the range.
    pub fn set_step(&self, step: f64) -> Result {
        self.edit(|control, fonts| {
            control.range.set_step(step)?;
            control.show(fonts)
        })
    }
    /// Shows at most 9 digits after the decimal point.
    pub fn set_decimals(&self, decimals: u8) -> Result {
        if decimals > 9 {
            return Err(UiError::InvalidValue.into());
        }
        self.edit(|control, fonts| {
            control.decimals = decimals;
            control.show(fonts)
        })
    }
    /// The displayed text, which may hold uncommitted typing.
    pub fn text(&self) -> Result<String> {
        self.change(|state, id| state.text(id))
    }
    /// Adds a user-change handler; handlers run in registration order outside tree borrows.
    pub fn on_change(&self, mut callback: impl FnMut(Self) -> Result + 'static) -> Result {
        self.change(|state, id| state.on_action(id, move |node| callback(Self(node))))
    }
    /// Removes the handlers and invalidates their queued invocations.
    pub fn clear_on_change(&self) -> Result {
        self.change(|state, id| {
            state.clear_actions(id);
            Ok(())
        })
    }
    /// Changes the control and its shown text.
    fn edit(
        &self,
        update: impl FnOnce(&mut NumberFieldControl, &mut TextSystem) -> Result,
    ) -> Result {
        self.change(|state, id| {
            let fonts = state.fonts.clone();
            update(
                state.control_as::<NumberFieldControl>(id).unwrap(),
                &mut fonts.borrow_mut(),
            )?;
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })
    }
}

impl Control for NumberFieldControl {
    fn kind(&self) -> ControlKind {
        ControlKind::TextField
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
    fn editor(&self) -> Option<&aegle_controls::TextField> {
        self.field.editor()
    }
    fn editor_mut(&mut self) -> Option<&mut aegle_controls::TextField> {
        self.field.editor_mut()
    }
    fn visual(&self) -> ControlVisual {
        self.field.visual()
    }
    fn set_enabled(&mut self, fonts: &mut TextSystem, enabled: bool) -> Outcome {
        self.field.set_enabled(fonts, enabled)
    }
    fn baseline(&self, size: Size, padding: f32) -> Option<f32> {
        self.field.baseline(size, padding)
    }
    fn content_offset(&self, size: Size, padding: f32, scroll: Point) -> Point {
        let offset = self.field.content_offset(size, padding, scroll);
        Point::new(offset.x - self.lead(), offset.y)
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let step = self.step();
        match input {
            Input::Pointer(pointer) if matches!(pointer.kind, PointerKind::Down { .. }) => {
                // Undo the field's content offset to find the stepper strip.
                let x = pointer.position.x - self.scroll.x + cx.padding + self.lead();
                let y = pointer.position.y - self.scroll.y + cx.padding;
                let on_strip = if self.rtl {
                    x < STRIP
                } else {
                    x >= cx.size.width - STRIP
                };
                if on_strip {
                    let up = y < cx.size.height * 0.5;
                    let value = self.range.value() + if up { step } else { -step };
                    return Ok(Outcome {
                        focus: true,
                        ..self.assign(cx.fonts, value)?
                    });
                }
            }
            Input::Key(key) if key.pressed => {
                let delta = match key.key {
                    Key::Up => Some(step),
                    Key::Down => Some(-step),
                    Key::PageUp => Some(step * 10.0),
                    Key::PageDown => Some(-step * 10.0),
                    Key::Enter => return self.commit(cx.fonts),
                    _ => None,
                };
                if let Some(delta) = delta {
                    return self.assign(cx.fonts, self.range.value() + delta);
                }
            }
            Input::Wheel { delta, .. } => {
                self.wheel += delta.x - delta.y;
                let mut steps = 0.0;
                while self.wheel.abs() >= WHEEL_STEP {
                    steps += f64::from(self.wheel.signum());
                    self.wheel -= WHEEL_STEP.copysign(self.wheel);
                }
                if steps == 0.0 {
                    return Ok(Outcome {
                        handled: true,
                        ..Outcome::default()
                    });
                }
                return self.assign(cx.fonts, self.range.value() + steps * step);
            }
            Input::Increment => return self.assign(cx.fonts, self.range.value() + step),
            Input::Decrement => return self.assign(cx.fonts, self.range.value() - step),
            Input::SetValue(value) => return self.assign(cx.fonts, value),
            Input::Focus(focused) => {
                self.focused = focused;
                self.wheel = 0.0;
                let outcome = self.field.handle(cx, input)?;
                let committed = if focused {
                    self.show(cx.fonts)?;
                    Outcome::default()
                } else {
                    self.commit(cx.fonts)?
                };
                return Ok(Outcome {
                    action: committed.action,
                    repaint: true,
                    reset_ime: true,
                    ..outcome
                });
            }
            _ => {}
        }
        self.field.handle(cx, input)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        let size = self.field.measure(&MeasureCx {
            fonts: cx.fonts,
            padding: cx.padding,
            gap: cx.gap,
            width: cx.width.map(|w| (w - STRIP).max(0.0)),
            rtl: cx.rtl,
        })?;
        Ok(Size::new(size.width + STRIP, size.height))
    }
    fn finalize(&mut self, cx: &MeasureCx<'_>) -> Result {
        self.rtl = cx.rtl;
        self.field.finalize(&MeasureCx {
            fonts: cx.fonts,
            padding: cx.padding,
            gap: cx.gap,
            width: cx.width.map(|w| (w - STRIP).max(0.0)),
            rtl: cx.rtl,
        })
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, root: bool, style: &mut Style) {
        self.field.retheme(theme, local, root, style);
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        self.scroll = cx.scroll;
        let (width, height) = (cx.size.width, cx.size.height);
        let shape = cx.shape;
        let lead = self.lead();
        let strip = Rect::new(
            if self.rtl { 0.0 } else { width - STRIP },
            0.0,
            STRIP,
            height,
        );
        // A one-pixel line on the strip's inner edge.
        let divider = if self.rtl { STRIP - 1.0 } else { width - STRIP };
        cx.builder.push_clip(RoundedRect::new(
            Rect::new(lead, 0.0, width - STRIP, height),
            0.0,
        )?)?;
        cx.builder.push_transform(Affine::translation(lead, 0.0)?)?;
        self.field.paint(cx)?;
        cx.builder.pop()?;
        cx.builder.pop()?;
        cx.builder.push_clip(shape)?;
        let color = cx.appearance.foreground;
        cx.builder.fill(
            RoundedRect::new(Rect::new(divider, 0.0, 1.0, height), 0.0)?,
            cx.appearance.border_color,
        )?;
        // Up and down chevrons centered in each half of the strip.
        for (center, up) in [(height * 0.25, true), (height * 0.75, false)] {
            let mut path = PathBuilder::new();
            let (x, half) = (strip.origin.x + STRIP * 0.5, 3.5);
            let tip = if up { -2.0 } else { 2.0 };
            path.move_to(ScenePoint::new(x - half, center - tip));
            path.line_to(ScenePoint::new(x, center + tip));
            path.line_to(ScenePoint::new(x + half, center - tip));
            cx.builder.stroke_path(
                &path.finish(aegle_scene::FillRule::NonZero)?,
                color,
                aegle_scene::Stroke::new(1.5),
            )?;
        }
        cx.builder.pop()?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Action, Role};
        cx.node.set_role(Role::SpinButton);
        cx.node.set_numeric_value(self.range.value());
        cx.node.set_min_numeric_value(self.range.min());
        cx.node.set_max_numeric_value(self.range.max());
        cx.node.set_numeric_value_step(self.step());
        if cx.enabled {
            cx.node.add_action(Action::Increment);
            cx.node.add_action(Action::Decrement);
            cx.node.add_action(Action::SetValue);
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

pub(crate) fn create(container: &Container, min: f64, max: f64, value: f64) -> Result<NumberField> {
    let range = Range::new(min, max, value, 0.0)?;
    crate::add(container, |state, theme| {
        let (field, style) = crate::field::control(state, theme, "", false)?;
        let mut control = NumberFieldControl {
            field,
            range,
            decimals: 0,
            focused: false,
            wheel: 0.0,
            scroll: Point::default(),
            rtl: false,
        };
        control.show(&mut state.fonts.borrow_mut())?;
        Ok((Box::new(control) as Box<dyn Control>, style))
    })
    .map(NumberField)
}
