//! How a control describes itself to assistive technology, in Aegle's own
//! vocabulary; the engine maps it to the platform accessibility schema.

use aegle_access::accesskit;

/// What a control is, for assistive technology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Role {
    /// A generic grouping without its own meaning.
    Group,
    /// Static text.
    Label,
    /// A picture.
    Image,
    /// A custom-drawn surface.
    Canvas,
    /// A push button.
    Button,
    /// A two- or three-state check box.
    CheckBox,
    /// An on/off switch.
    Switch,
    /// One choice of a radio group.
    RadioButton,
    /// A draggable value in a range.
    Slider,
    /// A numeric field with step buttons.
    SpinButton,
    /// A progress bar.
    ProgressIndicator,
    /// A divider that resizes the panes beside it.
    Splitter,
    /// A button opening a list of choices.
    ComboBox,
    /// A list of choices.
    ListBox,
    /// One choice in a list box.
    ListBoxOption,
    /// A menu.
    Menu,
    /// A bar of menus.
    MenuBar,
    /// A menu command.
    MenuItem,
    /// A menu command with a check mark.
    MenuItemCheckBox,
    /// A menu command among exclusive choices.
    MenuItemRadio,
    /// One tab of a tab list.
    Tab,
    /// A row of tabs.
    TabList,
    /// The content a tab shows.
    TabPanel,
    /// A table.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A column heading.
    ColumnHeader,
    /// A tree.
    Tree,
    /// One node of a tree.
    TreeItem,
    /// A dialog.
    Dialog,
    /// A navigation region, such as a drawer.
    Navigation,
}

impl Role {
    fn schema(self) -> accesskit::Role {
        use accesskit::Role as R;
        match self {
            Self::Group => R::GenericContainer,
            Self::Label => R::Label,
            Self::Image => R::Image,
            Self::Canvas => R::Canvas,
            Self::Button => R::Button,
            Self::CheckBox => R::CheckBox,
            Self::Switch => R::Switch,
            Self::RadioButton => R::RadioButton,
            Self::Slider => R::Slider,
            Self::SpinButton => R::SpinButton,
            Self::ProgressIndicator => R::ProgressIndicator,
            Self::Splitter => R::Splitter,
            Self::ComboBox => R::ComboBox,
            Self::ListBox => R::ListBox,
            Self::ListBoxOption => R::ListBoxOption,
            Self::Menu => R::Menu,
            Self::MenuBar => R::MenuBar,
            Self::MenuItem => R::MenuItem,
            Self::MenuItemCheckBox => R::MenuItemCheckBox,
            Self::MenuItemRadio => R::MenuItemRadio,
            Self::Tab => R::Tab,
            Self::TabList => R::TabList,
            Self::TabPanel => R::TabPanel,
            Self::Table => R::Table,
            Self::Row => R::Row,
            Self::Cell => R::Cell,
            Self::ColumnHeader => R::ColumnHeader,
            Self::Tree => R::Tree,
            Self::TreeItem => R::TreeItem,
            Self::Dialog => R::Dialog,
            Self::Navigation => R::Navigation,
        }
    }
}

/// An action assistive technology may ask a control to perform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SemanticAction {
    /// Activate it, like a click.
    Click,
    /// Give it keyboard focus.
    Focus,
    /// Step its value up.
    Increment,
    /// Step its value down.
    Decrement,
    /// Set its numeric value.
    SetValue,
}

impl SemanticAction {
    fn schema(self) -> accesskit::Action {
        match self {
            Self::Click => accesskit::Action::Click,
            Self::Focus => accesskit::Action::Focus,
            Self::Increment => accesskit::Action::Increment,
            Self::Decrement => accesskit::Action::Decrement,
            Self::SetValue => accesskit::Action::SetValue,
        }
    }

    /// The action an assistive-technology request names, if controls handle it.
    pub(crate) fn from_schema(action: accesskit::Action) -> Option<Self> {
        Some(match action {
            accesskit::Action::Click => Self::Click,
            accesskit::Action::Focus => Self::Focus,
            accesskit::Action::Increment => Self::Increment,
            accesskit::Action::Decrement => Self::Decrement,
            accesskit::Action::SetValue => Self::SetValue,
            _ => return None,
        })
    }
}

/// The state of a check box or toggle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggled {
    /// Off.
    False,
    /// On.
    True,
    /// Partly on, like a check box over a mixed group.
    Mixed,
}

/// Semantic export context for controls without an editor or viewport.
pub struct SemanticsCx<'a> {
    pub(crate) node: &'a mut accesskit::Node,
    /// Whether the control can currently be used.
    pub enabled: bool,
    /// Whether the application set an explicit accessible label.
    pub labelled: bool,
}

impl SemanticsCx<'_> {
    /// Sets what the control is.
    pub fn set_role(&mut self, role: Role) {
        self.node.set_role(role.schema());
    }
    /// Sets the name read for the control, unless the application labelled it.
    pub fn set_label(&mut self, label: &str) {
        self.node.set_label(label);
    }
    /// Sets a textual value, such as a chosen item.
    pub fn set_value(&mut self, value: &str) {
        self.node.set_value(value);
    }
    /// Sets a check box or toggle state.
    pub fn set_toggled(&mut self, toggled: Toggled) {
        self.node.set_toggled(match toggled {
            Toggled::False => accesskit::Toggled::False,
            Toggled::True => accesskit::Toggled::True,
            Toggled::Mixed => accesskit::Toggled::Mixed,
        });
    }
    /// Sets whether the control's children or popup are shown.
    pub fn set_expanded(&mut self, expanded: bool) {
        self.node.set_expanded(expanded);
    }
    /// Sets whether the control is the selected item of its container.
    pub fn set_selected(&mut self, selected: bool) {
        self.node.set_selected(selected);
    }
    /// Marks the control as opening a menu.
    pub fn set_opens_menu(&mut self) {
        self.node.set_has_popup(accesskit::HasPopup::Menu);
    }
    /// Sets the keyboard shortcut shown with the control, such as `Ctrl+S`.
    pub fn set_keyboard_shortcut(&mut self, shortcut: &str) {
        self.node.set_keyboard_shortcut(shortcut);
    }
    /// Sets whether the control is laid out vertically.
    pub fn set_vertical(&mut self, vertical: bool) {
        self.node.set_orientation(if vertical {
            accesskit::Orientation::Vertical
        } else {
            accesskit::Orientation::Horizontal
        });
    }
    /// Sets a numeric value and its range.
    pub fn set_numeric(&mut self, value: f64, min: f64, max: f64) {
        self.node.set_numeric_value(value);
        self.node.set_min_numeric_value(min);
        self.node.set_max_numeric_value(max);
    }
    /// Sets the step [`SemanticAction::Increment`] and `Decrement` move by.
    pub fn set_numeric_step(&mut self, step: f64) {
        self.node.set_numeric_value_step(step);
    }
    /// Declares an action the control supports.
    pub fn add_action(&mut self, action: SemanticAction) {
        self.node.add_action(action.schema());
    }
}
