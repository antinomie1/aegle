//! Handles to controls built from markup, and node properties applied
//! through the imperative setters.

use std::{any::Any, rc::Rc};

use aegle_markup::{PropertyName, Value as Literal};
use aegle_ui::{Color, ColorSlot, LengthSlot, Node, Result, Style, TokenSlot};
use aegle_widgets::NodeTooltip;

use crate::element::Glue;

/// A control created from markup, or the window of a `Window` document.
#[derive(Clone)]
pub struct Handle {
    pub(crate) node: Node,
    pub(crate) typed: Rc<dyn Any>,
    pub(crate) glue: Option<&'static dyn Glue>,
}

impl Handle {
    /// The control's node; a window's node is its root column.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// The typed handle: the element's [`Element::Handle`](crate::Element::Handle),
    /// or the window of a `Window` document root. `None` for another type.
    pub fn typed<T: Clone + 'static>(&self) -> Option<T> {
        self.typed.downcast_ref::<T>().cloned()
    }

    #[cfg(any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    pub(crate) fn window(window: aegle_app::Window) -> Self {
        Self {
            node: Node::clone(&window),
            typed: Rc::new(window),
            glue: None,
        }
    }

    /// A block's transparent group.
    pub(crate) fn group(group: aegle_ui::Container) -> Self {
        Self {
            node: group.0.clone(),
            typed: Rc::new(group),
            glue: None,
        }
    }

    /// Reads a checked `self` field inside an event handler.
    pub(crate) fn field(&self, name: &str) -> Result<crate::Data> {
        let glue = self.glue();
        let index = glue.spec().fields.iter().position(|(n, _)| *n == name);
        glue.get(self, index.expect("checked self field"))
    }

    pub(crate) fn glue(&self) -> &'static dyn Glue {
        self.glue.expect("an element's control")
    }
}

/// Node properties a window consumes when it opens, and timing properties
/// installed after every other property.
pub(crate) fn consumed(name: PropertyName) -> bool {
    use PropertyName::*;
    matches!(
        name,
        Title
            | Theme
            | Transition
            | Easing
            | PaintTransition
            | OffsetTransition
            | ScaleTransition
            | RotationTransition
    )
}

/// Applies one checked node property other than a window's or timing one.
pub fn apply(node: &Node, name: PropertyName, value: &Literal) -> Result {
    use PropertyName::*;
    let color = |value: &Literal| {
        let Literal::Color([r, g, b, a]) = *value else {
            unreachable!("checked color")
        };
        Color::rgba(r, g, b, a)
    };
    if let Literal::Call(function, arguments) = value
        && function == "token"
    {
        let [Literal::String(token)] = &arguments[..] else {
            unreachable!("checked token")
        };
        return bind_token(node, name, token);
    }
    if let Some(result) = crate::layout::apply(node, name, value) {
        return result;
    }
    if let Some(result) = crate::motion::geometry(node, name, value) {
        return result;
    }
    match (name, value) {
        (Grow, Literal::Number(n)) => node.set_grow(*n),
        (BorderWidth, Literal::Length(n)) => node.set_border_width(*n),
        (Radius, Literal::Length(n)) => node.set_radius(*n),
        (FocusWidth, Literal::Length(n)) => {
            style(node, LengthSlot::FocusWidth, |s| s.focus_width = Some(*n))
        }
        (FontSize, Literal::Length(n)) => node.change(|state, id| {
            state.write_unbound(id, LengthSlot::FontSize.into(), |state| {
                state.set_font_size(id, Some(*n))
            })
        }),
        (Visible, Literal::Bool(v)) => node.set_visible(*v),
        (Enabled, Literal::Bool(v)) => node.set_enabled(*v),
        (Label, Literal::String(text)) => node.set_accessible_label(text),
        (Tooltip, Literal::String(text)) => node.set_tooltip(Some(text)),
        (Background, value) => node.set_background(color(value)),
        (Foreground, value) => node.set_foreground(color(value)),
        (BorderColor, value) => node.set_border_color(color(value)),
        (FocusColor, value) => style(node, ColorSlot::FocusColor, |s| {
            s.focus_color = Some(color(value))
        }),
        (SelectionColor, value) => style(node, ColorSlot::Selection, |s| {
            s.selection = Some(color(value))
        }),
        (CaretColor, value) => style(node, ColorSlot::Caret, |s| s.caret = Some(color(value))),
        (HoverBackground, value) => style(node, ColorSlot::HoverBackground, |s| {
            s.hover_background = Some(color(value))
        }),
        (PressedBackground, value) => style(node, ColorSlot::PressedBackground, |s| {
            s.pressed_background = Some(color(value))
        }),
        (DisabledBackground, value) => node.set_disabled_background(color(value)),
        (DisabledForeground, value) => node.set_disabled_foreground(color(value)),
        (IndicatorColor, value) => style(node, ColorSlot::Indicator, |s| {
            s.indicator = Some(color(value))
        }),
        _ => unreachable!("checked property {name:?}"),
    }
}

/// Sets the style field the markup checker accepted on `node`.
fn style(node: &Node, slot: impl Into<TokenSlot>, edit: impl FnOnce(&mut Style)) -> Result {
    node.change(|state, id| state.set_style_field(id, slot.into(), edit))
}

/// Binds a checked property to a token looked up by name.
fn bind_token(node: &Node, name: PropertyName, token: &str) -> Result {
    use PropertyName::*;
    use aegle_ui::{ColorSlot as C, LengthSlot as L};
    let color = |slot| node.bind_color(slot, aegle_ui::token(token)?);
    let length = |slot| node.bind_length(slot, aegle_ui::token(token)?);
    match name {
        Background => color(C::Background),
        Foreground => color(C::Foreground),
        BorderColor => color(C::BorderColor),
        FocusColor => color(C::FocusColor),
        SelectionColor => color(C::Selection),
        CaretColor => color(C::Caret),
        HoverBackground => color(C::HoverBackground),
        PressedBackground => color(C::PressedBackground),
        DisabledBackground => color(C::DisabledBackground),
        DisabledForeground => color(C::DisabledForeground),
        IndicatorColor => color(C::Indicator),
        BorderWidth => length(L::BorderWidth),
        Radius => length(L::Radius),
        FocusWidth => length(L::FocusWidth),
        FontSize => length(L::FontSize),
        Padding => length(L::Padding),
        Gap => length(L::Gap),
        _ => unreachable!("checked token property"),
    }
}

pub use crate::motion::transitions;
