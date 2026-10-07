//! Containers that carry a semantic role for tables and popup lists.

use std::any::Any;

use aegle_layout::Style;
use aegle_theme::{Appearance, ControlKind, Theme, VisualState};
use aegle_ui::{Container, Control, Result, container_style};

/// The role a [`Group`] exports to assistive technology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// A floating column; `list` marks a dropdown's list of choices.
    Popup {
        list: bool,
    },
    Table,
    TableRow,
    TableCell,
    TableHeader,
    TabList,
    TabPanel,
}

/// A plain container with a semantic role.
pub struct Group {
    pub(crate) role: Role,
}

impl Control for Group {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn kind(&self) -> ControlKind {
        ControlKind::Container
    }
    fn retheme(&self, theme: &Theme, local: aegle_ui::LocalLayout, root: bool, style: &mut Style) {
        use aegle_layout::{Edges, LengthPercentage, Size};
        aegle_ui::Plain.retheme(theme, local, root, style);
        let popup = matches!(self.role, Role::Popup { .. });
        if (popup || matches!(self.role, Role::TableCell | Role::TableHeader))
            && !local.contains(aegle_ui::LocalLayout::PADDING)
        {
            let p = LengthPercentage::length(theme.padding / 2.0);
            style.padding = Edges {
                left: p,
                right: p,
                top: p,
                bottom: p,
            };
        }
        if popup && !local.contains(aegle_ui::LocalLayout::GAP) {
            let zero = LengthPercentage::length(0.0);
            style.gap = Size {
                width: zero,
                height: zero,
            };
        }
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        use aegle_ui::accesskit::Role as Access;
        match self.role {
            Role::Popup { list: true } => cx.node.set_role(Access::ListBox),
            Role::Popup { list: false } => {}
            Role::Table => cx.node.set_role(Access::Table),
            Role::TableRow => cx.node.set_role(Access::Row),
            Role::TableCell => cx.node.set_role(Access::Cell),
            Role::TableHeader => cx.node.set_role(Access::ColumnHeader),
            Role::TabList => cx.node.set_role(Access::TabList),
            Role::TabPanel => cx.node.set_role(Access::TabPanel),
        }
    }
}

/// A bordered surface for tables and popups, resolved from the current theme
/// so a theme change repaints it rather than keeping the creation-time colors.
pub(crate) fn panel(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.surface,
        border_color: theme.border,
        border_width: 1.0,
        ..Appearance::new(theme, state)
    }
}

/// A table header row: the window background inside the table's surface.
pub(crate) fn header(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.background,
        ..Appearance::new(theme, state)
    }
}

/// Appends a column or row (by `style`) with `role`.
pub(crate) fn add(container: &Container, role: Role, row: bool) -> Result<Container> {
    crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        if row {
            style.flex_direction = aegle_layout::FlexDirection::Row;
        }
        let group = Group { role };
        group.retheme(theme, aegle_ui::LocalLayout::NONE, false, &mut style);
        Ok((Box::new(group) as Box<dyn Control>, style))
    })
    .map(Container)
}
