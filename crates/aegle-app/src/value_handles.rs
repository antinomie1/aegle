use crate::{
    Container, Node, Result, UiError,
    handles::handle,
    state::{Content, Mark, State, ToggleContent},
};
use aegle_controls::{Input, Range, RangeError};
use aegle_core::{Dirty, NodeId};
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
    Radio,
    "A labeled choice that is exclusive among the radio buttons sharing its parent."
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
        /// Sets the value without invoking the user-change callback. A check
        /// box leaves the mixed state; checking a radio button unchecks its siblings.
        pub fn set_checked(&self, checked: bool) -> Result {
            self.change(|state, id| state.set_checked(id, checked))
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
toggles!(CheckBox, Switch, Radio);

impl CheckBox {
    /// Whether the box shows the mixed (partially checked) state.
    pub fn is_mixed(&self) -> Result<bool> {
        self.change(|state, id| {
            let Content::Toggle(toggle) = &state.tree.get(id).unwrap().context.content else {
                unreachable!()
            };
            Ok(toggle.mixed)
        })
    }
    /// Shows or leaves the mixed state without invoking the change handler.
    /// A user change from the mixed state checks the box.
    pub fn set_mixed(&self, mixed: bool) -> Result {
        self.change(|state, id| {
            let Content::Toggle(toggle) = &mut state.tree.get_mut(id).unwrap().context.content
            else {
                unreachable!()
            };
            if toggle.mixed != mixed {
                toggle.mixed = mixed;
                state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            Ok(())
        })
    }
}

impl State {
    pub fn set_checked(&mut self, id: NodeId, checked: bool) -> Result {
        let Content::Toggle(toggle) = &mut self.tree.get_mut(id).unwrap().context.content else {
            unreachable!()
        };
        let mut outcome = toggle.control.set_checked(checked);
        if std::mem::take(&mut toggle.mixed) {
            outcome.repaint = true;
            outcome.semantics = true;
        }
        let radio = toggle.mark == Mark::Radio;
        self.effects(id, outcome)?;
        if radio && checked {
            self.select_radio(id)?;
        }
        Ok(())
    }

    /// Arrow keys on a focused radio button focus and choose the previous or
    /// next enabled radio sibling, wrapping. Returns whether the key was used.
    pub fn radio_key(&mut self, key: &aegle_controls::KeyInput<'_>) -> Result<bool> {
        use aegle_controls::Key;
        let forward = match key.key {
            Key::Down | Key::Right => true,
            Key::Up | Key::Left => false,
            _ => return Ok(false),
        };
        let Some(id) = self.focus.current(&self.tree) else {
            return Ok(false);
        };
        let radio = |state: &State, node| {
            matches!(&state.tree.get(node).unwrap().context.content,
                Content::Toggle(toggle) if toggle.mark == Mark::Radio)
                && state.usable(node)
        };
        let Some(parent) = self.tree.parent(id)?.filter(|_| radio(self, id)) else {
            return Ok(false);
        };
        let group: Vec<_> = self
            .tree
            .children(parent)?
            .filter(|&n| radio(self, n))
            .collect();
        let index = group.iter().position(|&n| n == id).unwrap();
        let next = if forward {
            (index + 1) % group.len()
        } else {
            (index + group.len() - 1) % group.len()
        };
        self.set_focus(Some(group[next]))?;
        self.dispatch(group[next], Input::Activate)?;
        Ok(true)
    }

    /// Unchecks the other radio buttons sharing this one's parent.
    pub fn select_radio(&mut self, id: NodeId) -> Result {
        let Some(parent) = self.tree.parent(id)? else {
            return Ok(());
        };
        let siblings: Vec<_> = self.tree.children(parent)?.filter(|&n| n != id).collect();
        for sibling in siblings {
            if let Content::Toggle(toggle) =
                &mut self.tree.get_mut(sibling).unwrap().context.content
            {
                if toggle.mark == Mark::Radio && toggle.control.is_checked() {
                    let outcome = toggle.control.set_checked(false);
                    self.effects(sibling, outcome)?;
                }
            }
        }
        Ok(())
    }
}

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
        self.toggle_control(text, checked, Mark::Check)
            .map(CheckBox)
    }
    /// Appends a binary switch with a visible label.
    pub fn switch(&self, text: &str, checked: bool) -> Result<Switch> {
        self.toggle_control(text, checked, Mark::Switch).map(Switch)
    }
    /// Appends a radio button. Radio buttons sharing a parent form one group:
    /// checking one, by the user or [`Radio::set_checked`], unchecks the others.
    /// A newly created checked radio button unchecks its existing siblings.
    pub fn radio(&self, text: &str, checked: bool) -> Result<Radio> {
        let radio = self.toggle_control(text, checked, Mark::Radio)?;
        if checked {
            radio.change(|state, id| state.select_radio(id))?;
        }
        Ok(Radio(radio))
    }
    fn toggle_control(&self, text: &str, checked: bool, mark: Mark) -> Result<Node> {
        self.add(|state, theme| {
            let text = state
                .fonts
                .borrow_mut()
                .paragraph(text, &crate::state::text_style(theme))?;
            Ok((
                Content::Toggle(Box::new(ToggleContent {
                    control: aegle_controls::Toggle::new(checked),
                    text,
                    mark,
                    mixed: false,
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
