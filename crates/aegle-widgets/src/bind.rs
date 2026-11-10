//! Two-way bindings of input controls to signals.
//!
//! `control.bind_*(&signal)` shows the signal's value now and after each of
//! its changes, and writes the user's changes back to it, so that application
//! code reads and changes the model only. The control's programmatic setters do
//! not call change handlers, so the two directions do not feed each other, and
//! a value the control already shows is not set again: a text field being
//! edited keeps its caret, selection and undo history.

use aegle_ui::{Binding, Node, Signal, Subscription};

use crate::{CheckBox, Dropdown, NumberField, Radio, Slider, Switch, Tabs, TextField};

/// Binds `control` to `signal` in both directions; `read` and `show` access the
/// control's value, `watch` adds its user-change handler.
fn two_way<H, T>(
    control: &H,
    signal: &Signal<T>,
    read: fn(&H) -> T,
    show: fn(&H, &T),
    watch: impl FnOnce(&H, Box<dyn FnMut(H)>) -> Subscription,
) -> Binding
where
    H: AsRef<Node> + Clone + 'static,
    T: Clone + PartialEq + 'static,
{
    let binding = signal.bind(control, move |control, value| {
        if read(control) != *value {
            show(control, value);
        }
    });
    let model = signal.clone();
    let subscription = watch(control, Box::new(move |control| model.set(read(&control))));
    binding.with_subscription(subscription)
}

macro_rules! bind {
    ($method:ident($value:ty) $what:literal $note:literal, $read:expr, $show:expr; $($ty:ident),+) => {$(
        impl $ty {
            #[doc = concat!("Shows `signal` as the ", $what, " now and after each change of")]
            /// the signal, and sets the signal to the user's changes, until the
            /// control is removed or [`Binding::remove`]; see [`Signal::bind`].
            #[doc = $note]
            pub fn $method(&self, signal: &Signal<$value>) -> Binding {
                two_way(self, signal, $read, $show, |control, mut handler| {
                    control.on_change(move |control| handler(control))
                })
            }
        }
    )+};
}

bind!(bind_checked(bool) "checked state" "", |c| c.is_checked(), |c, v| c.set_checked(*v); CheckBox, Switch);
bind!(bind_text(String) "committed text" "", |c| c.text(), |c, v| c.set_text(v); TextField);
bind!(
    bind_value(f64) "value"
    "\nThe control clamps a value outside its range; the signal keeps the value it was given until the user changes the control.",
    |c| c.value(), |c, v| c.set_value(*v); Slider, NumberField
);
bind!(bind_selected(usize) "selected item" "", |c| c.selected(), |c, v| c.set_selected(*v); Dropdown);
bind!(bind_selected(usize) "selected tab" "", |c| c.selected(), |c, v| c.select(*v); Tabs);

impl Radio {
    /// Checks this radio button while `signal` equals `value`, and sets
    /// `signal` to `value` when the user chooses it, until the control is
    /// removed or [`Binding::remove`]; see [`Signal::bind`].
    ///
    /// Bind each radio button of a group (the radio buttons sharing a parent)
    /// to the same signal with a distinct value. Checking one unchecks its
    /// siblings, and a signal value that no button has leaves them all
    /// unchecked.
    ///
    /// ```
    /// # fn group(parent: &aegle_ui::Container) {
    /// use aegle_ui::Signal;
    /// use aegle_widgets::Widgets;
    ///
    /// #[derive(Clone, Copy, PartialEq)]
    /// enum Size { Small, Large }
    ///
    /// let size = Signal::new(Size::Small);
    /// parent.radio("Small", false).bind_choice(&size, Size::Small);
    /// parent.radio("Large", false).bind_choice(&size, Size::Large);
    /// # }
    /// ```
    pub fn bind_choice<T: Clone + PartialEq + 'static>(
        &self,
        signal: &Signal<T>,
        value: T,
    ) -> Binding {
        let choice = value.clone();
        let binding = signal.bind(self, move |radio, current| {
            let chosen = *current == choice;
            if radio.is_checked() != chosen {
                radio.set_checked(chosen);
            }
        });
        // Only the radio button the user newly chooses reports a change.
        let model = signal.clone();
        binding.with_subscription(self.on_change(move |_| model.set(value.clone())))
    }
}
