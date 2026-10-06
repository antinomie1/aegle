//! Containers that carry a semantic role for tables and popup lists.

use std::any::Any;

use aegle_layout::Style;
use aegle_theme::{ControlKind, Theme};
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
    fn retheme(&self, theme: &Theme, local: u8, root: bool, style: &mut Style) {
        aegle_ui::Plain.retheme(theme, local, root, style);
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
        }
    }
}

/// Appends a column or row (by `style`) with `role`.
pub(crate) fn add(container: &Container, role: Role, row: bool) -> Result<Container> {
    crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        if row {
            style.flex_direction = aegle_layout::FlexDirection::Row;
        }
        Ok((Box::new(Group { role }) as Box<dyn Control>, style))
    })
    .map(Container)
}
