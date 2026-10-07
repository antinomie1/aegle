//! Typed control handles and the mapping from markup properties to setters.

use std::rc::Rc;

use aegle_markup::{Bound, Element, EventKind, Kind, PropertyName, Step, Value as Literal};
use aegle_ui::{Color, Container, Node, Result};
use aegle_widgets::{
    Button, CheckBox, Label, NodeTooltip, NumberField, Orientation, Progress, Radio, ScrollView,
    Separator, Slider, Splitter, Switch, Tabs, TextField, Widgets,
};

use crate::{Data, eval::Env, eval::handle as run};

/// A typed handle to a control created from markup.
#[derive(Clone)]
pub enum Handle {
    /// The native window of a Window document root.
    #[cfg(any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    Window(aegle_app::Window),
    /// Column or Row.
    Container(Container),
    /// ScrollView.
    ScrollView(ScrollView),
    /// Text.
    Label(Label),
    /// Button.
    Button(Button),
    /// TextField or TextArea.
    TextField(TextField),
    /// CheckBox.
    CheckBox(CheckBox),
    /// Switch.
    Switch(Switch),
    /// RadioButton.
    Radio(Radio),
    /// Slider.
    Slider(Slider),
    /// Progress.
    Progress(Progress),
    /// Separator.
    Separator(Separator),
    /// NumberField.
    NumberField(NumberField),
    /// Tabs; each Tab page is a [`Handle::Container`].
    Tabs(Tabs),
    /// Splitter.
    Splitter(Splitter),
}

/// Converts a [`Handle`] to the typed handle of its control, as generated views do.
pub trait FromHandle: Sized {
    /// The typed handle, or `None` for another control kind.
    fn from_handle(handle: &Handle) -> Option<Self>;
}

macro_rules! from_handle {
    ($($variant:ident($ty:ty)),*) => {$(
        impl FromHandle for $ty {
            fn from_handle(handle: &Handle) -> Option<Self> {
                match handle {
                    Handle::$variant(handle) => Some(handle.clone()),
                    #[allow(unreachable_patterns)]
                    _ => None,
                }
            }
        }
    )*};
}

from_handle!(
    Container(Container),
    ScrollView(ScrollView),
    Label(Label),
    Button(Button),
    TextField(TextField),
    CheckBox(CheckBox),
    Switch(Switch),
    Radio(Radio),
    Slider(Slider),
    Progress(Progress),
    Separator(Separator),
    NumberField(NumberField),
    Tabs(Tabs),
    Splitter(Splitter)
);
#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
from_handle!(Window(aegle_app::Window));

impl Handle {
    /// The control's node; a window's node is its root column.
    pub fn node(&self) -> &Node {
        match self {
            #[cfg(any(
                all(feature = "wayland", target_os = "linux"),
                all(feature = "windows", target_os = "windows")
            ))]
            Self::Window(window) => window,
            Self::Container(handle) => handle,
            Self::ScrollView(handle) => handle,
            Self::Label(handle) => handle,
            Self::Button(handle) => handle,
            Self::TextField(handle) => handle,
            Self::CheckBox(handle) => handle,
            Self::Switch(handle) => handle,
            Self::Radio(handle) => handle,
            Self::Slider(handle) => handle,
            Self::Progress(handle) => handle,
            Self::Separator(handle) => handle,
            Self::NumberField(handle) => handle,
            Self::Tabs(handle) => handle,
            Self::Splitter(handle) => handle,
        }
    }

    pub(crate) fn container(&self) -> &Container {
        match self {
            #[cfg(any(
                all(feature = "wayland", target_os = "linux"),
                all(feature = "windows", target_os = "windows")
            ))]
            Self::Window(window) => window,
            Self::Container(handle) => handle,
            Self::ScrollView(handle) => handle,
            Self::Tabs(handle) => handle,
            Self::Splitter(handle) => handle,
            _ => unreachable!("checked: only containers have children"),
        }
    }

    /// Reads a checked `self` field inside an event handler.
    pub(crate) fn field(&self, field: &str) -> Result<Data> {
        Ok(match (self, field) {
            (Self::CheckBox(handle), "checked") => Data::Bool(handle.is_checked()?),
            (Self::Switch(handle), "checked") => Data::Bool(handle.is_checked()?),
            (Self::Radio(handle), "checked") => Data::Bool(handle.is_checked()?),
            (Self::Radio(handle), _) => Data::String(handle.text()?.into()),
            (Self::CheckBox(handle), _) => Data::String(handle.text()?.into()),
            (Self::Switch(handle), _) => Data::String(handle.text()?.into()),
            (Self::TextField(handle), _) => Data::String(handle.text()?.into()),
            (Self::Slider(handle), _) => Data::Float(handle.value()? as f32),
            (Self::NumberField(handle), _) => Data::Float(handle.value()? as f32),
            (Self::Tabs(handle), _) => Data::Int(handle.selected()? as i64),
            _ => unreachable!("checked self fields"),
        })
    }
}

/// Literal properties consumed by a constructor or by the transition step.
pub(crate) fn consumed(kind: Kind, name: PropertyName) -> bool {
    use PropertyName::*;
    matches!(
        name,
        Title
            | Text
            | Checked
            | Min
            | Max
            | Value
            | Transition
            | Easing
            | PaintTransition
            | OffsetTransition
            | ScaleTransition
            | RotationTransition
    ) || (kind == Kind::Window && matches!(name, Width | Height))
        || (kind == Kind::Splitter && name == Orientation)
}

fn literal<'a>(element: &'a Element, name: PropertyName) -> Option<&'a Literal> {
    element
        .properties
        .iter()
        .find_map(|(n, bound)| match bound {
            Bound::Literal(value) if *n == name => Some(value),
            _ => None,
        })
}

fn orientation(element: &Element) -> Orientation {
    literal(element, PropertyName::Orientation).map_or(Orientation::Horizontal, orientation_of)
}

fn orientation_of(value: &Literal) -> Orientation {
    match crate::layout::identifier(value) {
        "horizontal" => Orientation::Horizontal,
        "vertical" => Orientation::Vertical,
        other => unreachable!("checked orientation `{other}`"),
    }
}

/// Creates a non-window control with its literal constructor arguments.
pub(crate) fn create(kind: Kind, element: &Element, parent: &Container) -> Result<Handle> {
    let text = match literal(element, PropertyName::Text)
        .or_else(|| literal(element, PropertyName::Title))
    {
        Some(Literal::String(text)) => text.as_str(),
        _ => "",
    };
    let checked = matches!(
        literal(element, PropertyName::Checked),
        Some(Literal::Bool(true))
    );
    let number = |name, default| match literal(element, name) {
        Some(Literal::Number(value)) => f64::from(*value),
        _ => default,
    };
    let range = (
        number(PropertyName::Min, 0.0),
        number(PropertyName::Max, 1.0),
        number(PropertyName::Value, 0.0),
    );
    Ok(match kind {
        Kind::Column => Handle::Container(parent.column()?),
        Kind::Row => Handle::Container(parent.row()?),
        #[cfg(feature = "grid")]
        Kind::Grid => Handle::Container(parent.grid(&[])?),
        #[cfg(feature = "grid")]
        Kind::Stack => Handle::Container(parent.stack()?),
        #[cfg(not(feature = "grid"))]
        Kind::Grid | Kind::Stack => {
            return Err("markup Grid and Stack require the grid feature".into());
        }
        Kind::ScrollView => Handle::ScrollView(parent.scroll_view()?),
        Kind::Text => Handle::Label(parent.text(text)?),
        Kind::Button => Handle::Button(parent.button(text)?),
        Kind::TextField => Handle::TextField(parent.text_field(text)?),
        Kind::TextArea => Handle::TextField(parent.text_area(text)?),
        Kind::CheckBox => Handle::CheckBox(parent.check_box(text, checked)?),
        Kind::Switch => Handle::Switch(parent.switch(text, checked)?),
        Kind::RadioButton => Handle::Radio(parent.radio(text, checked)?),
        Kind::Slider => Handle::Slider(parent.slider(range.0, range.1, range.2)?),
        Kind::Progress => Handle::Progress(parent.progress(range.0, range.1, range.2)?),
        Kind::NumberField => Handle::NumberField(parent.number_field(range.0, range.1, range.2)?),
        Kind::Separator => Handle::Separator(parent.separator()?),
        Kind::Tabs => Handle::Tabs(parent.tabs()?),
        Kind::Tab => Handle::Container(Tabs(parent.clone()).add(text)?),
        Kind::Splitter => Handle::Splitter(parent.splitter(orientation(element))?),
        Kind::Window => unreachable!("windows are opened from the App"),
    })
}

/// Applies one checked property value through the imperative setters.
pub(crate) fn apply(handle: &Handle, name: PropertyName, value: &Literal) -> Result {
    use PropertyName::*;
    let node = handle.node();
    let color = |value: &Literal| {
        let Literal::Color([r, g, b, a]) = *value else {
            unreachable!("checked color")
        };
        Color::rgba(r, g, b, a)
    };
    let container = match handle {
        Handle::Container(_) | Handle::ScrollView(_) | Handle::Tabs(_) | Handle::Splitter(_) => {
            Some(handle.container())
        }
        #[cfg(any(
            all(feature = "wayland", target_os = "linux"),
            all(feature = "windows", target_os = "windows")
        ))]
        Handle::Window(_) => Some(handle.container()),
        _ => None,
    };
    if let Literal::Call(function, arguments) = value
        && function == "token"
    {
        let [Literal::String(token)] = &arguments[..] else {
            unreachable!("checked token")
        };
        return bind_token(node, name, token);
    }
    if let Some(result) = crate::layout::apply(node, container, name, value) {
        return result;
    }
    if let Some(result) = crate::motion::geometry(node, name, value) {
        return result;
    }
    match (name, value) {
        (Grow, Literal::Number(n)) => node.set_grow(*n),
        (BorderWidth, Literal::Length(n)) => node.set_border_width(*n),
        (Radius, Literal::Length(n)) => node.set_radius(*n),
        (FocusWidth, Literal::Length(n)) => node.set_focus_width(*n),
        (FontSize, Literal::Length(n)) => node.set_font_size(*n),
        (Visible, Literal::Bool(v)) => node.set_visible(*v),
        (Enabled, Literal::Bool(v)) => node.set_enabled(*v),
        (Label, Literal::String(text)) => node.set_accessible_label(text),
        (Tooltip, Literal::String(text)) => node.set_tooltip(Some(text)),
        (Background, value) => node.set_background(color(value)),
        (Foreground, value) => node.set_foreground(color(value)),
        (BorderColor, value) => node.set_border_color(color(value)),
        (FocusColor, value) => node.set_focus_color(color(value)),
        (SelectionColor, value) => node.set_selection_color(color(value)),
        (CaretColor, value) => node.set_caret_color(color(value)),
        (HoverBackground, value) => node.set_hover_background(color(value)),
        (PressedBackground, value) => node.set_pressed_background(color(value)),
        (DisabledBackground, value) => node.set_disabled_background(color(value)),
        (DisabledForeground, value) => node.set_disabled_foreground(color(value)),
        (IndicatorColor, value) => node.set_indicator_color(color(value)),
        (Text, Literal::String(text)) => match handle {
            Handle::Label(handle) => handle.set_text(text),
            Handle::Button(handle) => handle.set_text(text),
            Handle::TextField(handle) => handle.set_text(text),
            Handle::CheckBox(handle) => handle.set_text(text),
            Handle::Switch(handle) => handle.set_text(text),
            Handle::Radio(handle) => handle.set_text(text),
            _ => unreachable!("checked text property"),
        },
        (Checked, Literal::Bool(v)) => match handle {
            Handle::CheckBox(handle) => handle.set_checked(*v),
            Handle::Switch(handle) => handle.set_checked(*v),
            Handle::Radio(handle) => handle.set_checked(*v),
            _ => unreachable!("checked toggle property"),
        },
        (Value, Literal::Number(n)) => match handle {
            Handle::Slider(handle) => handle.set_value(f64::from(*n)),
            Handle::Progress(handle) => handle.set_value(f64::from(*n)),
            Handle::NumberField(handle) => handle.set_value(f64::from(*n)),
            _ => unreachable!("checked range property"),
        },
        (Orientation, Literal::Identifier(_)) => {
            let orientation = orientation_of(value);
            match handle {
                Handle::Slider(handle) => handle.set_orientation(orientation),
                Handle::Progress(handle) => handle.set_orientation(orientation),
                _ => unreachable!("checked orientation property"),
            }
        }
        (Indeterminate, Literal::Bool(v)) => match handle {
            Handle::Progress(handle) => handle.set_indeterminate(*v),
            _ => unreachable!("checked indeterminate property"),
        },
        (Decimals, Literal::Number(n)) => match handle {
            Handle::NumberField(handle) => handle.set_decimals(*n as u8),
            _ => unreachable!("checked decimals property"),
        },
        (Ratio, Literal::Number(n) | Literal::Percent(n)) => match handle {
            Handle::Splitter(handle) => handle.set_ratio(if matches!(value, Literal::Percent(_)) {
                n / 100.0
            } else {
                *n
            }),
            _ => unreachable!("checked ratio property"),
        },
        (Mixed, Literal::Bool(v)) => match handle {
            Handle::CheckBox(handle) => handle.set_mixed(*v),
            _ => unreachable!("checked mixed property"),
        },
        (Step, Literal::Number(n)) => match handle {
            Handle::Slider(handle) => handle.set_step(f64::from(*n)),
            Handle::NumberField(handle) => handle.set_step(f64::from(*n)),
            _ => unreachable!("checked step property"),
        },
        (ReadOnly | Password, Literal::Bool(v)) => match handle {
            Handle::TextField(handle) if name == ReadOnly => handle.set_read_only(*v),
            Handle::TextField(handle) => handle.set_password(*v),
            _ => unreachable!("checked editor property"),
        },
        #[cfg(any(
            all(feature = "wayland", target_os = "linux"),
            all(feature = "windows", target_os = "windows")
        ))]
        (Theme, Literal::Identifier(theme)) => {
            let Handle::Window(window) = handle else {
                unreachable!("checked: themes apply to windows")
            };
            window.set_theme(match theme.as_str() {
                "light" => aegle_ui::Theme::light(),
                "dark" => aegle_ui::Theme::dark(),
                "high_contrast" => aegle_ui::Theme::high_contrast(),
                other => unreachable!("checked theme `{other}`"),
            })
        }
        _ => unreachable!("checked property {name:?}"),
    }
}

/// Installs an event block; it runs outside the UI borrow like any handler.
pub(crate) fn listen(handle: &Handle, event: EventKind, steps: Rc<[Step]>, env: Env) -> Result {
    match (event, handle) {
        (EventKind::Clicked, Handle::Button(button)) => {
            button.on_click(move |button| run(&steps, &env, &Handle::Button(button)))
        }
        (EventKind::Changed, Handle::CheckBox(control)) => {
            control.on_change(move |control| run(&steps, &env, &Handle::CheckBox(control)))
        }
        (EventKind::Changed, Handle::Switch(control)) => {
            control.on_change(move |control| run(&steps, &env, &Handle::Switch(control)))
        }
        (EventKind::Changed, Handle::Radio(control)) => {
            control.on_change(move |control| run(&steps, &env, &Handle::Radio(control)))
        }
        (EventKind::Changed, Handle::Slider(control)) => {
            control.on_change(move |control| run(&steps, &env, &Handle::Slider(control)))
        }
        (EventKind::Changed, Handle::NumberField(control)) => {
            control.on_change(move |control| run(&steps, &env, &Handle::NumberField(control)))
        }
        (EventKind::Changed, Handle::Tabs(control)) => {
            control.on_change(move |control| run(&steps, &env, &Handle::Tabs(control)))
        }
        (EventKind::Submitted, Handle::TextField(field)) => {
            field.on_submit(move |field| run(&steps, &env, &Handle::TextField(field)))
        }
        _ => unreachable!("checked event kinds"),
    }
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
