//! The built-in elements, declared with [`element!`](crate::element) exactly
//! like a control library's.

use aegle_ui::Container;
use aegle_widgets::{
    Button, CheckBox, Label, NumberField, Orientation, Progress, Radio, Separator, Slider,
    Splitter, Switch, Tabs, TextField, Widgets,
};

use crate::Elements;

fn axis(name: &str) -> Orientation {
    match name {
        "horizontal" => Orientation::Horizontal,
        "vertical" => Orientation::Vertical,
        other => unreachable!("checked orientation `{other}`"),
    }
}

#[cfg(feature = "grid")]
fn grid(parent: &Container, stack: bool) -> Container {
    if stack { parent.stack() } else { parent.grid() }
}

#[cfg(not(feature = "grid"))]
fn grid(_: &Container, _: bool) -> Container {
    panic!("markup Grid and Stack require the grid feature")
}

aegle_macros::element! {
    /// A vertical flex container.
    pub Column(Container) {
        layout flex;
        create |parent| parent.column();
    }
    /// A horizontal flex container.
    pub Row(Container) {
        layout flex;
        create |parent| parent.row();
    }
    /// A clipped, scrollable vertical container.
    pub ScrollView(Container) {
        layout flex;
        create |parent| parent.scroll_view();
    }
    /// A grid container; requires the `grid` feature.
    pub Grid(Container) {
        layout grid;
        create |parent| grid(parent, false);
    }
    /// Children overlapping in one cell; requires the `grid` feature.
    pub Stack(Container) {
        layout box;
        create |parent| grid(parent, true);
    }
    /// Static text.
    pub Text(Label) {
        style text;
        create |parent, text: string = ""| parent.text(text);
        set text: string => |label, text| label.set_text(text);
    }
    /// A push button.
    pub Button {
        style text interactive pressed;
        create |parent, text: string = ""| parent.button(text);
        set text: string => |button, text| button.set_text(text);
        event clicked => |button, run| button.on_click(move |_| run());
    }
    /// A single-line editor.
    pub TextField {
        style text interactive editor;
        create |parent, text: line = ""| parent.text_field(text);
        set text: line => |field, text| field.set_text(text);
        set read_only: bool => |field, on| field.set_read_only(on);
        set password: bool => |field, on| field.set_password(on);
        event submitted => |field, run| field.on_submit(move |_| run());
        get text: string => |field| field.text();
    }
    /// A multiline editor.
    pub TextArea(TextField) {
        style text interactive editor;
        create |parent, text: string = ""| parent.text_area(text);
        set text: string => |field, text| field.set_text(text);
        set read_only: bool => |field, on| field.set_read_only(on);
    }
    /// A labelled two-state check box.
    pub CheckBox {
        style text interactive pressed indicator;
        create |parent, text: string = "", checked: bool = false| parent.check_box(text, checked);
        set text: string => |toggle, text| toggle.set_text(text);
        set checked: bool => |toggle, on| toggle.set_checked(on);
        set mixed: bool => |toggle, on| toggle.set_mixed(on);
        event changed => |toggle, run| toggle.on_change(move |_| run());
        get checked: bool => |toggle| toggle.is_checked();
        get text: string => |toggle| toggle.text();
    }
    /// A labelled two-state switch.
    pub Switch {
        style text interactive pressed indicator;
        create |parent, text: string = "", checked: bool = false| parent.switch(text, checked);
        set text: string => |toggle, text| toggle.set_text(text);
        set checked: bool => |toggle, on| toggle.set_checked(on);
        event changed => |toggle, run| toggle.on_change(move |_| run());
        get checked: bool => |toggle| toggle.is_checked();
        get text: string => |toggle| toggle.text();
    }
    /// A labelled choice, exclusive among its sibling radio buttons.
    pub RadioButton(Radio) {
        style text interactive pressed indicator;
        create |parent, text: string = "", checked: bool = false| parent.radio(text, checked);
        set text: string => |toggle, text| toggle.set_text(text);
        set checked: bool => |toggle, on| toggle.set_checked(on);
        event changed => |toggle, run| toggle.on_change(move |_| run());
        get checked: bool => |toggle| toggle.is_checked();
        get text: string => |toggle| toggle.text();
    }
    /// An interactive numeric range; the constructor requires `min < max`.
    pub Slider {
        style interactive pressed indicator;
        create |parent, min: float = 0, max: float = 1, value: float = 0| parent.slider(min, max, value);
        set value: float => |slider, value| slider.set_value(value);
        set step: float(0) => |slider, step| slider.set_step(step);
        set orientation: choice(horizontal, vertical) => |slider, name| slider.set_orientation(axis(name));
        event changed => |slider, run| slider.on_change(move |_| run());
        get value: float => |slider| slider.value();
    }
    /// A noninteractive numeric progress indicator.
    pub Progress {
        style indicator;
        create |parent, min: float = 0, max: float = 1, value: float = 0| parent.progress(min, max, value);
        set value: float => |progress, value| progress.set_value(value);
        set orientation: choice(horizontal, vertical) => |progress, name| progress.set_orientation(axis(name));
        set indeterminate: bool => |progress, on| progress.set_indeterminate(on);
    }
    /// A numeric text field with steppers.
    pub NumberField {
        style text interactive editor;
        create |parent, min: float = 0, max: float = 1, value: float = 0| parent.number_field(min, max, value);
        set value: float => |field, value| field.set_value(value);
        set step: float(0) => |field, step| field.set_step(step);
        set decimals: int(0, 9) => |field, decimals| field.set_decimals(decimals as u8);
        event changed => |field, run| field.on_change(move |_| run());
        get value: float => |field| field.value();
    }
    /// A one-pixel divider.
    pub Separator {
        create |parent| parent.separator();
    }
    /// A tab list whose children are `Tab` pages.
    pub Tabs {
        layout box;
        children only Tab;
        create |parent| parent.tabs();
        event changed => |tabs, run| tabs.on_change(move |_| run());
        get selected: int => |tabs| tabs.selected() as i64;
    }
    /// One page of a `Tabs`, titled by `title`.
    pub Tab(Container) {
        layout box;
        parent Tabs;
        create |parent, title: line| Tabs(parent.0.clone()).add(title);
    }
    /// Two panes, its two children, divided by a draggable handle.
    pub Splitter {
        layout box;
        children exactly 2;
        create |parent, orientation: choice(horizontal, vertical) = "horizontal"| parent.splitter(axis(orientation));
        set ratio: fraction => |splitter, ratio| splitter.set_ratio(ratio);
        children => |splitter, child| (if child == 0 { splitter.first() } else { splitter.second() }).clone();
    }
}

/// Every built-in element.
pub(crate) fn builtin() -> Elements {
    let elements = Elements(Vec::new());
    elements
        .with::<Column>()
        .with::<Row>()
        .with::<ScrollView>()
        .with::<Grid>()
        .with::<Stack>()
        .with::<Text>()
        .with::<Button>()
        .with::<TextField>()
        .with::<TextArea>()
        .with::<CheckBox>()
        .with::<Switch>()
        .with::<RadioButton>()
        .with::<Slider>()
        .with::<Progress>()
        .with::<NumberField>()
        .with::<Separator>()
        .with::<Tabs>()
        .with::<Tab>()
        .with::<Splitter>()
}
