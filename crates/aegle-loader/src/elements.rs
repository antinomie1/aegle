//! The built-in elements, declared with [`element!`](crate::element) exactly
//! like a control library's.

use aegle_ui::Container;
use aegle_widgets::{
    Button, CheckBox, Dialog, Drawer, DrawerSide, Dropdown, Label, NumberField, Orientation,
    Progress, Radio, Separator, Slider, Splitter, Switch, Tabs, TextField, TreeNode, TreeView,
    Widgets,
};

use crate::Elements;

fn axis(name: &str) -> Orientation {
    match name {
        "horizontal" => Orientation::Horizontal,
        "vertical" => Orientation::Vertical,
        other => unreachable!("checked orientation `{other}`"),
    }
}

fn drawer_side(name: &str) -> DrawerSide {
    match name {
        "left" => DrawerSide::Left,
        "right" => DrawerSide::Right,
        other => unreachable!("checked side `{other}`"),
    }
}

/// New choices keep the selected index while it remains valid, else the first.
fn set_items(dropdown: &Dropdown, items: crate::Lines<'_>) {
    let items = items.to_vec();
    let selected = Some(dropdown.selected()).filter(|&index| index < items.len());
    dropdown.set_items(&items, selected.unwrap_or(0))
}

#[cfg(feature = "grid")]
fn grid(parent: &Container, stack: bool) -> Container {
    if stack { parent.stack() } else { parent.grid() }
}

#[cfg(not(feature = "grid"))]
fn grid(_: &Container, _: bool) -> Container {
    panic!("markup Grid and Stack require the grid feature")
}

crate::element! {
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
        set wrap: bool => |label, on| label.set_wrap(on);
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
        event changed => |field, run| field.on_change(move |_| run());
        get text: string => |field| field.text();
    }
    /// A multiline editor.
    pub TextArea(TextField) {
        style text interactive editor;
        create |parent, text: string = ""| parent.text_area(text);
        set text: string => |field, text| field.set_text(text);
        set read_only: bool => |field, on| field.set_read_only(on);
        event changed => |field, run| field.on_change(move |_| run());
        get text: string => |field| field.text();
    }
    /// A labelled two-state check box.
    pub CheckBox {
        style text interactive pressed indicator;
        create |parent, text: string = "", checked: bool = false| parent.check_box(text, checked);
        set text: string => |toggle, text| toggle.set_text(text);
        set wrap: bool => |toggle, on| toggle.set_wrap(on);
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
        set wrap: bool => |toggle, on| toggle.set_wrap(on);
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
        set wrap: bool => |toggle, on| toggle.set_wrap(on);
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
    /// A button opening a list of `items`; `selected` must index them.
    pub Dropdown {
        style text interactive pressed;
        create |parent, items: lines| parent.dropdown(&items.to_vec(), 0);
        set items: lines => |dropdown, items| set_items(dropdown, items);
        set selected: int(0, 4294967295) => |dropdown, index| dropdown.set_selected(index as usize);
        event changed => |dropdown, run| dropdown.on_change(move |_| run());
        get selected: int => |dropdown| dropdown.selected() as i64;
        get text: string => |dropdown| dropdown.text();
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
    /// A hierarchical list whose children are `TreeNode`s.
    pub TreeView {
        layout box;
        children blocks TreeNode;
        create |parent| parent.tree_view();
    }
    /// One expandable node of a `TreeView`; its children are nested nodes.
    pub TreeNode {
        layout box;
        parent TreeView TreeNode;
        children blocks TreeNode;
        create |parent, text: line = ""| TreeNode::append_to(parent, text);
        set text: line => |node, text| node.set_text(text);
        set expanded: bool => |node, on| node.set_expanded(on);
        set selected: bool => |node, on| node.set_selected(on);
        event selected => |node, run| node.on_select(move |_| run());
        get text: string => |node| node.text();
        get expanded: bool => |node| node.is_expanded();
        get selected: bool => |node| node.is_selected();
        children => |node, _| node.children_container();
    }
    /// A modal dialog over its parent; children go in its body.
    pub Dialog {
        layout box;
        create |parent, title: line = ""| parent.dialog(title);
        set title: line => |dialog, title| dialog.set_title(title);
        set open: bool => |dialog, on| match (on, dialog.is_open()) {
            (true, false) => dialog.show(),
            (false, true) => dialog.close(),
            _ => {}
        };
        set dismissible: bool => |dialog, on| dialog.set_dismissible(on);
        event closed => |dialog, run| dialog.on_close(run);
        get open: bool => |dialog| dialog.is_open();
        children => |dialog, _| dialog.body();
    }
    /// A navigation panel docked to an edge of its parent; children go in its panel.
    pub Drawer {
        layout box;
        create |parent, side: choice(left, right) = "left"| parent.drawer_on(drawer_side(side));
        set open: bool => |drawer, on| match (on, drawer.is_open()) {
            (true, false) => drawer.open(),
            (false, true) => drawer.close(),
            _ => {}
        };
        set width: length => |drawer, width| drawer.set_width(width);
        event closed => |drawer, run| drawer.on_close(run);
        get open: bool => |drawer| drawer.is_open();
        children => |drawer, _| drawer.content();
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
        .with::<Dropdown>()
        .with::<NumberField>()
        .with::<Separator>()
        .with::<Tabs>()
        .with::<Tab>()
        .with::<Splitter>()
        .with::<TreeView>()
        .with::<TreeNode>()
        .with::<Dialog>()
        .with::<Drawer>()
}
