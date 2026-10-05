use crate::{
    Container, Node, Result, UiError,
    handles::handle,
    state::{Content, ToggleContent},
};
use aegle_controls::{Input, Range, RangeError};
use aegle_core::Dirty;
use aegle_layout::{Dimension, Style};
use std::ops::Deref;

handle!(
    CheckBox,
    "A retained binary checkbox with a text label and shared activation."
);
handle!(
    Switch,
    "A retained binary switch with a text label and shared activation."
);
handle!(
    Slider,
    "A horizontal numeric slider with pointer, keyboard and semantic adjustment."
);
handle!(
    Progress,
    "A determinate horizontal progress indicator; it is not focusable."
);

macro_rules! toggles {
    ($($ty:ident),*) => { $(impl $ty {
        /// Returns the retained binary value.
        pub fn is_checked(&self) -> Result<bool> {
            self.change(|state, id| {
                let Content::Toggle(toggle) = &state.tree.get(id).unwrap().context.content else { unreachable!() };
                Ok(toggle.control.is_checked())
            })
        }
        /// Sets the value without invoking the user-change callback.
        pub fn set_checked(&self, checked: bool) -> Result {
            self.change(|state, id| {
                let Content::Toggle(toggle) = &mut state.tree.get_mut(id).unwrap().context.content else { unreachable!() };
                let outcome = toggle.control.set_checked(checked);
                state.effects(id, outcome)
            })
        }
        /// Toggles through the same enabled/visible behavior as user activation.
        pub fn toggle(&self) -> Result {
            self.change(|state, id| state.dispatch(id, Input::Activate))
        }
        /// Replaces the visible label and its default accessible name.
        pub fn set_text(&self, text: &str) -> Result { self.0.set_text(text) }
        /// Copies the visible label.
        pub fn text(&self) -> Result<String> { self.0.text() }
        /// Replaces the user-change handler. It runs outside tree borrows and can
        /// query the latest value; programmatic setters do not invoke it.
        pub fn on_change(&self, mut callback: impl FnMut(Self) -> Result + 'static) -> Result {
            self.0.on_action(move |node| callback(Self(node)))
        }
        /// Removes the change handler and invalidates its queued invocations.
        pub fn clear_on_change(&self) -> Result { self.0.clear_on_action() }
    })* };
}
toggles!(CheckBox, Switch);

macro_rules! ranges {
    ($($ty:ident),*) => { $(impl $ty {
        /// Returns the retained numeric value.
        pub fn value(&self) -> Result<f64> { self.0.read_range(|range| range.value()) }
        /// Returns the inclusive minimum and maximum.
        pub fn range(&self) -> Result<(f64, f64)> { self.0.read_range(|range| (range.min(), range.max())) }
        /// Sets a finite value, clamped to the bounds and slider step grid.
        /// Does not invoke the user-change callback.
        pub fn set_value(&self, value: f64) -> Result {
            self.0.update_range(|range| range.set_value(value))
        }
        /// Replaces finite increasing bounds with finite span, then clamps the
        /// current value. Invalid bounds leave the old range unchanged.
        pub fn set_range(&self, min: f64, max: f64) -> Result {
            self.0.update_range(|range| range.set_bounds(min, max))
        }
    })* };
}
ranges!(Slider, Progress);

impl Slider {
    /// Sets a finite nonnegative step anchored at the minimum; zero is continuous.
    /// The maximum is always reachable, including when it is not on the step grid.
    pub fn set_step(&self, step: f64) -> Result {
        self.0.update_range(|range| range.set_step(step))
    }
    /// Returns the step; zero means continuous pointer adjustment.
    pub fn step(&self) -> Result<f64> {
        self.0.read_range(|range| range.step())
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

impl Node {
    fn read_range<T>(&self, read: impl FnOnce(&Range) -> T) -> Result<T> {
        self.change(|state, id| {
            let range = match &state.tree.get(id).unwrap().context.content {
                Content::Slider(slider) => slider.range(),
                Content::Progress(range) => range,
                _ => return Err(UiError::WrongKind.into()),
            };
            Ok(read(range))
        })
    }
    fn update_range(
        &self,
        update: impl FnOnce(&mut Range) -> std::result::Result<bool, RangeError>,
    ) -> Result {
        self.change(|state, id| {
            let range = match &mut state.tree.get_mut(id).unwrap().context.content {
                Content::Slider(slider) => slider.range_mut(),
                Content::Progress(range) => range,
                _ => return Err(UiError::WrongKind.into()),
            };
            if update(range)? {
                state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            Ok(())
        })
    }
}

impl Container {
    /// Appends a binary checkbox; the text is also its default accessible name.
    pub fn check_box(&self, text: &str, checked: bool) -> Result<CheckBox> {
        self.toggle_control(text, checked, false).map(CheckBox)
    }
    /// Appends a binary switch with a visible label.
    pub fn switch(&self, text: &str, checked: bool) -> Result<Switch> {
        self.toggle_control(text, checked, true).map(Switch)
    }
    fn toggle_control(&self, text: &str, checked: bool, switch: bool) -> Result<Node> {
        self.add(|state, theme| {
            let text = state
                .fonts
                .borrow_mut()
                .paragraph(text, &crate::state::text_style(theme))?;
            Ok((
                Content::Toggle(Box::new(ToggleContent {
                    control: aegle_controls::Toggle::new(checked),
                    text,
                    switch,
                })),
                control_style(theme.control_height),
            ))
        })
    }
    /// Appends a continuous horizontal slider. Bounds must have a finite positive
    /// span; finite initial values clamp to them. Use set_step for discrete steps.
    pub fn slider(&self, min: f64, max: f64, value: f64) -> Result<Slider> {
        let range = Range::new(min, max, value, 0.0)?;
        self.add(|_, theme| {
            Ok((
                Content::Slider(Box::new(aegle_controls::Slider::new(range))),
                control_style(theme.control_height),
            ))
        })
        .map(Slider)
    }
    /// Appends a determinate progress bar with finite increasing bounds.
    pub fn progress(&self, min: f64, max: f64, value: f64) -> Result<Progress> {
        let range = Range::new(min, max, value, 0.0)?;
        self.add(|_, theme| {
            Ok((
                Content::Progress(range),
                control_style(theme.control_height / 2.0),
            ))
        })
        .map(Progress)
    }
}

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
