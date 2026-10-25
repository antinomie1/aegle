//! Containers that carry a semantic role for tables and popup lists.

use aegle_theme::{Appearance, ControlKind, Theme, VisualState};
use aegle_ui::LayoutStyle;
use aegle_ui::{Container, Control, container_style};

/// The role a [`Group`] exports to assistive technology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// A floating column; `list` marks a dropdown's list of choices.
    Popup {
        list: bool,
    },
    /// A popup of menu items.
    Menu,
    MenuBar,
    Table,
    TableRow,
    TableCell,
    TableHeader,
    TabList,
    TabPanel,
    Dialog,
    Drawer,
}

/// A plain container with a semantic role.
pub struct Group {
    pub(crate) role: Role,
}

impl Group {
    /// Popups, menus, dialog cards and drawer panels float over the window and
    /// scroll their content when it does not fit there.
    fn floats(&self) -> bool {
        matches!(
            self.role,
            Role::Popup { .. } | Role::Menu | Role::Dialog | Role::Drawer
        )
    }
}

impl Control for Group {
    fn kind(&self) -> &'static ControlKind {
        match self.role {
            Role::Popup { .. } | Role::Menu | Role::Table => &crate::kinds::PANEL,
            Role::Dialog => &crate::kinds::DIALOG,
            Role::Drawer => &crate::kinds::DRAWER,
            Role::MenuBar => &crate::kinds::MENU_BAR,
            _ => &aegle_ui::CONTAINER,
        }
    }
    fn retheme(
        &self,
        theme: &Theme,
        local: aegle_ui::LocalLayout,
        root: bool,
        style: &mut LayoutStyle,
    ) {
        aegle_ui::Plain.retheme(theme, local, root, style);
        let popup = matches!(self.role, Role::Popup { .. } | Role::Menu);
        if (popup || matches!(self.role, Role::TableCell | Role::TableHeader))
            && !local.contains(aegle_ui::LocalLayout::PADDING)
        {
            style.set_padding(theme.padding / 2.0);
        }
        if (popup || self.role == Role::MenuBar) && !local.contains(aegle_ui::LocalLayout::GAP) {
            style.set_gap(0.0, 0.0);
        }
    }
    fn viewport(&self) -> bool {
        self.floats()
    }
    fn frame(&self) -> aegle_ui::control::Frame {
        // A floating surface's border draws after its children, so scrolled
        // content cannot cover it.
        aegle_ui::control::Frame {
            background: true,
            border: !self.floats(),
        }
    }
    fn paint_overlay(&mut self, cx: &mut aegle_ui::control::PaintCx<'_>) {
        aegle_ui::paint::outline(
            cx.builder,
            cx.size,
            cx.appearance.radius,
            cx.appearance.border_width,
            cx.appearance.border_color,
        );
        aegle_ui::bar::paint(cx.builder, cx.bars, cx.bar_color);
    }
    fn paint(&mut self, cx: &mut aegle_ui::control::PaintCx<'_>) {
        // A tab list stands on a divider line its selected tab's underline
        // covers; table rows are ruled off by the same line.
        if matches!(self.role, Role::TabList | Role::TableRow) {
            let line = aegle_scene::Rect::new(0.0, cx.size.height - 1.0, cx.size.width, 1.0);
            cx.builder
                .fill(aegle_scene::RoundedRect::new(line, 0.0), cx.theme.divider);
        }
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::Role as Access;
        match self.role {
            Role::Popup { list: true } => cx.set_role(Access::ListBox),
            Role::Popup { list: false } => {}
            Role::Menu => cx.set_role(Access::Menu),
            Role::MenuBar => cx.set_role(Access::MenuBar),
            Role::Table => cx.set_role(Access::Table),
            Role::TableRow => cx.set_role(Access::Row),
            Role::TableCell => cx.set_role(Access::Cell),
            Role::TableHeader => cx.set_role(Access::ColumnHeader),
            Role::TabList => cx.set_role(Access::TabList),
            Role::TabPanel => cx.set_role(Access::TabPanel),
            Role::Dialog => cx.set_role(Access::Dialog),
            Role::Drawer => cx.set_role(Access::Navigation),
        }
    }
}

/// A table header row: the window background inside the table's surface.
pub(crate) fn header(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.background,
        ..Appearance::base(theme, state)
    }
}

/// Binds a floating surface's shadow to [`Theme::shadow`], so it follows
/// theme changes; an application's `set_shadow` replaces the binding.
pub(crate) fn elevate(state: &mut aegle_ui::State, id: aegle_core::NodeId) -> aegle_ui::Result {
    state.bind_token(id, aegle_ui::TokenSlot::Shadow, Theme::SHADOW.index())
}

/// Appends a column or row (by `style`) with `role`.
pub(crate) fn add(container: &Container, role: Role, row: bool) -> Container {
    Container(crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        if row {
            style.set_direction(aegle_ui::Direction::Row);
        }
        let group = Group { role };
        group.retheme(theme, aegle_ui::LocalLayout::NONE, false, &mut style);
        Ok((Box::new(group) as Box<dyn Control>, style))
    }))
}
