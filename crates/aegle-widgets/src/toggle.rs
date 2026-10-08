//! Check boxes, switches and radio buttons.

use std::any::Any;

use aegle_controls::{Action, Input, Key, KeyInput, Outcome};
use aegle_core::{Dirty, NodeId};
use aegle_layout::{Dimension, Style};
use aegle_text::{Paragraph, TextSystem};
use aegle_theme::{ControlKind, Theme};
use aegle_types::Size;
use aegle_ui::{
    Container, Control, Node, Result, State,
    control::{ControlVisual, InputCx, MeasureCx, PaintCx},
    handle, text_style,
};

use crate::paint::{Mark, ToggleSpec, toggle};

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

macro_rules! toggles {
    ($($ty:ident),*) => { $(impl $ty {
        /// Returns the retained binary value.
        pub fn is_checked(&self) -> Result<bool> {
            self.change(|state, id| {
                let toggle = state.control_as::<ToggleControl>(id).expect("a toggle node");
                Ok(toggle.control.is_checked())
            })
        }
        /// Sets the value without invoking the user-change callback. A check
        /// box leaves the mixed state; checking a radio button unchecks its siblings.
        pub fn set_checked(&self, checked: bool) -> Result {
            self.change(|state, id| set_checked(state, id, checked))
        }
        /// Toggles through the same enabled/visible behavior as user activation.
        pub fn toggle(&self) -> Result {
            self.change(|state, id| state.dispatch(id, Input::Activate))
        }
        /// Replaces the visible label and its default accessible name.
        pub fn set_text(&self, text: &str) -> Result {
            self.change(|state, id| state.set_text(id, text))
        }
        /// Copies the visible label.
        pub fn text(&self) -> Result<String> {
            self.change(|state, id| state.text(id))
        }
        /// Adds a user-change handler. Handlers run in registration order
        /// outside tree borrows and can query the latest value; programmatic
        /// setters do not invoke them.
        pub fn on_change(&self, mut callback: impl FnMut(Self) -> Result + 'static) -> Result {
            self.change(|state, id| state.on_action(id, move |node| callback(Self(node))))
        }
        /// Removes the change handlers and invalidates their queued invocations.
        pub fn clear_on_change(&self) -> Result {
            self.change(|state, id| Ok(state.clear_actions(id)))
        }
    })* };
}
toggles!(CheckBox, Switch, Radio);

impl CheckBox {
    /// Whether the box shows the mixed (partially checked) state.
    pub fn is_mixed(&self) -> Result<bool> {
        self.change(|state, id| {
            Ok(state
                .control_as::<ToggleControl>(id)
                .expect("a toggle node")
                .mixed)
        })
    }
    /// Shows or leaves the mixed state without invoking the change handler.
    /// A user change from the mixed state checks the box.
    pub fn set_mixed(&self, mixed: bool) -> Result {
        self.change(|state, id| {
            let toggle = state
                .control_as::<ToggleControl>(id)
                .expect("a toggle node");
            if toggle.mixed != mixed {
                toggle.mixed = mixed;
                state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            Ok(())
        })
    }
}

/// The control inside a check box, switch or radio button node.
pub struct ToggleControl {
    control: aegle_controls::Toggle,
    text: Paragraph,
    mark: Mark,
    /// A check box shown as partially checked until the user changes it.
    mixed: bool,
}

impl Control for ToggleControl {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        match self.mark {
            Mark::Check => ControlKind::CheckBox,
            Mark::Switch => ControlKind::Switch,
            Mark::Radio => ControlKind::RadioButton,
        }
    }
    fn interactive(&self) -> bool {
        true
    }
    fn self_clipping(&self) -> bool {
        true
    }
    fn uses_gap(&self) -> bool {
        true
    }
    fn frame(&self) -> aegle_ui::control::Frame {
        aegle_ui::control::Frame {
            background: true,
            border: false,
        }
    }
    fn paragraph(&self) -> Option<&Paragraph> {
        Some(&self.text)
    }
    fn paragraph_mut(&mut self) -> Option<&mut Paragraph> {
        Some(&mut self.text)
    }
    fn visual(&self) -> ControlVisual {
        ControlVisual {
            pressed: self.control.is_pressed(),
            hovered: Some(self.control.is_hovered()),
            checked: self.control.is_checked() && !self.mixed,
            ..Default::default()
        }
    }
    fn set_enabled(&mut self, _: &mut TextSystem, enabled: bool) -> Outcome {
        self.control.set_enabled(enabled)
    }
    fn handle(&mut self, cx: &mut InputCx<'_>, input: Input<'_>) -> Result<Outcome> {
        let was = self.control.is_checked();
        let mut outcome = self.control.handle(input);
        if outcome.action == Some(Action::Change) {
            if self.mark == Mark::Radio {
                // Choosing the chosen radio button changes nothing.
                self.control.set_checked(true);
                let chosen = !was;
                outcome.action = chosen.then_some(Action::Change);
                if chosen {
                    cx.deferred.push(Box::new(select_radio));
                }
            } else if std::mem::take(&mut self.mixed) {
                self.control.set_checked(true);
            }
        }
        Ok(outcome)
    }
    fn hover(
        &mut self,
        cx: &mut InputCx<'_>,
        _: aegle_ui::PointerId,
        input: Input<'_>,
    ) -> Result<Outcome> {
        self.handle(cx, input)
    }
    fn baseline(&self, size: Size, _: f32) -> Option<f32> {
        // The label is centered vertically beside the marker.
        if self.text.text().is_empty() {
            return None;
        }
        Some((size.height - self.text.size().height) / 2.0 + self.text.first_baseline()?)
    }
    fn measure(&mut self, cx: &MeasureCx<'_>) -> Result<Size> {
        let marker = if self.mark == Mark::Switch {
            36.0
        } else {
            18.0
        };
        let label = if self.text.text().is_empty() {
            0.0
        } else {
            cx.gap + self.text.size().width
        };
        Ok(Size::new(
            marker + label + 2.0 * cx.padding,
            self.text.size().height.max(20.0) + 2.0 * cx.padding,
        ))
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, _: bool, style: &mut Style) {
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            style.size.height = Dimension::length(theme.control_height);
        }
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        Ok(toggle(
            cx.builder,
            &ToggleSpec {
                size: cx.size,
                padding: cx.padding,
                gap: cx.theme.gap,
                mark: self.mark,
                checked: self.control.is_checked(),
                mixed: self.mixed,
                label: (!self.text.text().is_empty()).then(|| self.text.size()),
                rtl: cx.rtl,
            },
            *cx.appearance,
            |builder, color| self.text.paint_with_color(builder, color),
        )?)
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::{Action, Role, Toggled};
        cx.node.set_role(match self.mark {
            Mark::Check => Role::CheckBox,
            Mark::Switch => Role::Switch,
            Mark::Radio => Role::RadioButton,
        });
        cx.node.set_toggled(if self.mixed {
            Toggled::Mixed
        } else if self.control.is_checked() {
            Toggled::True
        } else {
            Toggled::False
        });
        if !cx.labelled {
            cx.node.set_label(self.text.text());
        }
        if cx.enabled {
            cx.node.add_action(Action::Focus);
            cx.node.add_action(Action::Click);
        }
    }
    #[cfg(feature = "accessibility")]
    fn action_input(
        &self,
        action: aegle_ui::accesskit::Action,
        _: Option<&aegle_ui::accesskit::ActionData>,
    ) -> Option<Input<'static>> {
        (action == aegle_ui::accesskit::Action::Click).then_some(Input::Activate)
    }
}

fn set_checked(state: &mut State, id: NodeId, checked: bool) -> Result {
    let toggle = state
        .control_as::<ToggleControl>(id)
        .expect("a toggle node");
    let mut outcome = toggle.control.set_checked(checked);
    if std::mem::take(&mut toggle.mixed) {
        outcome.repaint = true;
        outcome.semantics = true;
    }
    let radio = toggle.mark == Mark::Radio;
    state.effects(id, outcome)?;
    if radio && checked {
        select_radio(state, id)?;
    }
    Ok(())
}

/// Unchecks the other radio buttons sharing this one's parent.
pub(crate) fn select_radio(state: &mut State, id: NodeId) -> Result {
    let Some(parent) = state.tree.parent(id)? else {
        return Ok(());
    };
    let siblings: Vec<_> = state.tree.children(parent)?.filter(|&n| n != id).collect();
    for sibling in siblings {
        if let Some(toggle) = state.control_as::<ToggleControl>(sibling)
            && toggle.mark == Mark::Radio
            && toggle.control.is_checked()
        {
            let outcome = toggle.control.set_checked(false);
            state.effects(sibling, outcome)?;
        }
    }
    Ok(())
}

/// Arrow keys on a focused radio button focus and choose the previous or next
/// enabled radio sibling, wrapping; Left moves forward right to left. Returns
/// whether the key was used.
pub(crate) fn radio_key(state: &mut State, key: &KeyInput<'_>) -> Result<bool> {
    let Some(id) = state.focus.current(&state.tree) else {
        return Ok(false);
    };
    let forward = match key.key {
        Key::Down => true,
        Key::Up => false,
        Key::Right => !state.rtl(id),
        Key::Left => state.rtl(id),
        _ => return Ok(false),
    };
    let radio = |state: &mut State, node| {
        state
            .control_as::<ToggleControl>(node)
            .is_some_and(|toggle| toggle.mark == Mark::Radio)
            && state.usable(node)
    };
    let Some(parent) = state.tree.parent(id)?.filter(|_| radio(state, id)) else {
        return Ok(false);
    };
    let children: Vec<_> = state.tree.children(parent)?.collect();
    let group: Vec<_> = children.into_iter().filter(|&n| radio(state, n)).collect();
    let index = group.iter().position(|&n| n == id).unwrap();
    let next = if forward {
        (index + 1) % group.len()
    } else {
        (index + group.len() - 1) % group.len()
    };
    state.set_focus(Some(group[next]))?;
    state.dispatch(group[next], Input::Activate)?;
    Ok(true)
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

fn create(container: &Container, text: &str, checked: bool, mark: Mark) -> Result<Node> {
    crate::add(container, |state, theme| {
        let text = state
            .fonts
            .borrow_mut()
            .paragraph(text, &text_style(theme))?;
        Ok((
            Box::new(ToggleControl {
                control: aegle_controls::Toggle::new(checked),
                text,
                mark,
                mixed: false,
            }) as Box<dyn Control>,
            control_style(theme.control_height),
        ))
    })
}

pub(crate) fn check_box(container: &Container, text: &str, checked: bool) -> Result<CheckBox> {
    create(container, text, checked, Mark::Check).map(CheckBox)
}

pub(crate) fn switch(container: &Container, text: &str, checked: bool) -> Result<Switch> {
    create(container, text, checked, Mark::Switch).map(Switch)
}

pub(crate) fn radio(container: &Container, text: &str, checked: bool) -> Result<Radio> {
    let radio = create(container, text, checked, Mark::Radio)?;
    if checked {
        radio.change(select_radio)?;
    }
    Ok(Radio(radio))
}
