//! Sliders and determinate progress bars.

use std::any::Any;

use aegle_controls::{Input, Outcome, Range, RangeError};
use aegle_core::Dirty;
use aegle_layout::{Dimension, Style};
use aegle_text::TextSystem;
use aegle_theme::{ControlKind, Theme};
use aegle_types::{Point, Size};
use aegle_ui::{
    Container, Control, Node, Result, UiError,
    control::{ControlVisual, Frame, InputCx, MeasureCx, PaintCx, StyleScope},
    handle,
};

use crate::paint::{range, slider_track};

handle!(
    Slider,
    "A horizontal numeric slider with pointer, keyboard and semantic adjustment."
);
handle!(
    Progress,
    "A determinate horizontal progress indicator; it is not focusable."
);

macro_rules! ranges {
    ($($ty:ident),*) => { $(impl $ty {
        /// Returns the retained numeric value.
        pub fn value(&self) -> Result<f64> { read_range(&self.0, |range| range.value()) }
        /// Returns the inclusive minimum and maximum.
        pub fn range(&self) -> Result<(f64, f64)> { read_range(&self.0, |range| (range.min(), range.max())) }
        /// Sets a finite value, clamped to the bounds and slider step grid.
        /// Does not invoke the user-change callback.
        pub fn set_value(&self, value: f64) -> Result {
            update_range(&self.0, |range| range.set_value(value))
        }
        /// Replaces finite increasing bounds with finite span, then clamps the
        /// current value. Invalid bounds leave the old range unchanged.
        pub fn set_range(&self, min: f64, max: f64) -> Result {
            update_range(&self.0, |range| range.set_bounds(min, max))
        }
    })* };
}
ranges!(Slider, Progress);

impl Slider {
    /// Sets a finite nonnegative step anchored at the minimum; zero is continuous.
    /// The maximum is always reachable, including when it is not on the step grid.
    pub fn set_step(&self, step: f64) -> Result {
        update_range(&self.0, |range| range.set_step(step))
    }
    /// Returns the step; zero means continuous pointer adjustment.
    pub fn step(&self) -> Result<f64> {
        read_range(&self.0, |range| range.step())
    }
    /// Requests one enabled user increment, including a change callback if changed.
    pub fn increment(&self) -> Result {
        self.change(|state, id| state.dispatch(id, Input::Increment))
    }
    /// Requests one enabled user decrement, including a change callback if changed.
    pub fn decrement(&self) -> Result {
        self.change(|state, id| state.dispatch(id, Input::Decrement))
    }
    /// Replaces the user-change handler, called outside tree borrows. The handle
    /// exposes the latest value; pending notifications are not value snapshots.
    pub fn on_change(&self, mut callback: impl FnMut(Self) -> Result + 'static) -> Result {
        self.0.on_action(move |node| callback(Self(node)))
    }
    /// Removes the handler and invalidates its queued invocations.
    pub fn clear_on_change(&self) -> Result {
        self.0.clear_on_action()
    }
}

fn read_range<T>(node: &Node, read: impl FnOnce(&Range) -> T) -> Result<T> {
    node.change(|state, id| {
        let control = &mut state.tree.get_mut(id).unwrap().context.control;
        let range = if let Some(slider) = control.as_any_mut().downcast_mut::<SliderControl>() {
            slider.0.range()
        } else if let Some(progress) = control.as_any_mut().downcast_mut::<ProgressControl>() {
            &progress.0
        } else {
            return Err(UiError::WrongKind.into());
        };
        Ok(read(range))
    })
}

fn update_range(
    node: &Node,
    update: impl FnOnce(&mut Range) -> std::result::Result<bool, RangeError>,
) -> Result {
    node.change(|state, id| {
        let control = &mut state.tree.get_mut(id).unwrap().context.control;
        let range = if let Some(slider) = control.as_any_mut().downcast_mut::<SliderControl>() {
            slider.0.range_mut()
        } else if let Some(progress) = control.as_any_mut().downcast_mut::<ProgressControl>() {
            &mut progress.0
        } else {
            return Err(UiError::WrongKind.into());
        };
        if update(range)? {
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
        Ok(())
    })
}

/// The control inside a [`Slider`] node.
pub struct SliderControl(Box<aegle_controls::Slider>);

/// The control inside a [`Progress`] node.
pub struct ProgressControl(Range);

fn control_style(height: f32) -> Style {
    Style {
        size: aegle_layout::Size {
            width: Dimension::auto(),
            height: Dimension::length(height),
        },
        flex_shrink: 0.0,
        ..Default::default()
    }
}

#[cfg(feature = "accessibility")]
fn numeric(node: &mut aegle_ui::accesskit::Node, range: &Range) {
    node.set_numeric_value(range.value());
    node.set_min_numeric_value(range.min());
    node.set_max_numeric_value(range.max());
}

impl Control for SliderControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Slider
    }
    fn interactive(&self) -> bool {
        true
    }
    fn drags(&self) -> bool {
        true
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn style_scope(&self) -> StyleScope {
        StyleScope {
            button_like: true,
            indicator: true,
            ..Default::default()
        }
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn visual(&self) -> ControlVisual {
        ControlVisual {
            pressed: self.0.is_pressed(),
            hovered: Some(self.0.is_hovered()),
            ..Default::default()
        }
    }
    fn set_enabled(&mut self, _: &mut TextSystem, enabled: bool) -> Outcome {
        self.0.set_enabled(enabled)
    }
    fn content_offset(&self, size: Size, padding: f32, _: Point) -> Point {
        Point::new(-slider_track(size, padding).0, 0.0)
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let (_, extent) = slider_track(cx.size, cx.padding);
        Ok(self.0.handle(input, extent)?)
    }
    fn hover(
        &mut self,
        _: &mut InputCx<'_>,
        pointer: aegle_ui::PointerId,
        _: Input<'_>,
    ) -> Result<Outcome> {
        Ok(self.0.update_hover(pointer, true))
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        Ok(Size::new(160.0, 20.0 + 2.0 * cx.padding))
    }
    fn retheme(&self, theme: &Theme, local: u8, _: bool, style: &mut Style) {
        if local & 1 == 0 {
            style.size.height = Dimension::length(theme.control_height);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        range(
            cx.builder,
            cx.size,
            cx.padding,
            self.0.range().fraction(),
            true,
            *cx.appearance,
        )?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Action, Orientation, Role};
        cx.node.set_role(Role::Slider);
        cx.node.set_orientation(Orientation::Horizontal);
        let range = self.0.range();
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
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Progress
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn style_scope(&self) -> StyleScope {
        StyleScope {
            indicator: true,
            ..Default::default()
        }
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        Ok(Size::new(160.0, 20.0 + 2.0 * cx.padding))
    }
    fn retheme(&self, theme: &Theme, local: u8, _: bool, style: &mut Style) {
        if local & 1 == 0 {
            style.size.height = Dimension::length(theme.control_height / 2.0);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        range(
            cx.builder,
            cx.size,
            cx.padding,
            self.0.fraction(),
            false,
            *cx.appearance,
        )?;
        Ok(())
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.node
            .set_role(aegle_ui::accesskit::Role::ProgressIndicator);
        numeric(cx.node, &self.0);
    }
}

pub(crate) fn slider(container: &Container, min: f64, max: f64, value: f64) -> Result<Slider> {
    let range = Range::new(min, max, value, 0.0)?;
    crate::add(container, |_, theme| {
        Ok((
            Box::new(SliderControl(Box::new(aegle_controls::Slider::new(range))))
                as Box<dyn Control>,
            control_style(theme.control_height),
        ))
    })
    .map(Slider)
}

pub(crate) fn progress(container: &Container, min: f64, max: f64, value: f64) -> Result<Progress> {
    let range = Range::new(min, max, value, 0.0)?;
    crate::add(container, |_, theme| {
        Ok((
            Box::new(ProgressControl(range)) as Box<dyn Control>,
            control_style(theme.control_height / 2.0),
        ))
    })
    .map(Progress)
}
