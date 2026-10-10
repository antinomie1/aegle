//! A header row over a virtual list of fixed-height rows.

use std::ops::Deref;

use aegle_layout::Overflow;
use aegle_ui::{Container, Node, Result, UiError};

use crate::{
    ListView, Widgets,
    group::{self, Role},
};

/// A table column: header text and a fixed logical width, or `None` to share
/// the remaining width equally with the other flexible columns.
#[derive(Clone, Copy, Debug)]
pub struct TableColumn<'a> {
    /// Header text, also the column's accessible name.
    pub title: &'a str,
    /// Fixed finite nonnegative width, or `None` to grow.
    pub width: Option<f32>,
}

/// A bordered table: one header row and virtual rows built on demand.
/// It dereferences to its outer node; [`Table::rows`] scrolls and resizes.
#[derive(Clone)]
pub struct Table {
    table: Node,
    rows: ListView,
}

impl Deref for Table {
    type Target = Node;
    fn deref(&self) -> &Node {
        &self.table
    }
}

impl Table {
    /// The virtual row list: `set_count`, `reload`, scrolling and its size.
    pub fn rows(&self) -> &ListView {
        &self.rows
    }
}

/// A header or body cell sized by its column.
fn cell(line: &Container, column: &TableColumn, role: Role) -> Container {
    let cell = group::add(line, role, false);
    match column.width {
        Some(width) => cell.set_width(Some(width)),
        None => cell.set_grow(1.0),
    }
    cell
}

/// See [`Widgets::table`].
pub(crate) fn table(
    container: &Container,
    columns: &[TableColumn],
    row_height: f32,
    rows: usize,
    mut fill: impl FnMut(&Container, usize, usize) -> Result + 'static,
) -> Table {
    let valid = |width: Option<f32>| width.is_none_or(|w| w.is_finite() && w >= 0.0);
    if columns.is_empty() || !columns.iter().all(|c| valid(c.width)) {
        panic!("{}", UiError::InvalidValue);
    }
    let table = group::add(container, Role::Table, false);
    table.set_gap(0.0, 0.0);
    // Keeps the header fill inside the border.
    table.set_padding(1.0);
    // A clipping box has no content-based minimum size, so the virtual rows'
    // full extent never stops the table from shrinking to the space it is given.
    table.change(|state, id| {
        let mut style = state.tree.get(id).unwrap().style().clone();
        style.overflow.x = Overflow::Hidden;
        style.overflow.y = Overflow::Hidden;
        Ok(aegle_layout::set_style(&mut state.tree, id, style)?)
    });
    let header = group::add(&table, Role::TableRow, true);
    header.set_gap(0.0, 0.0);
    header.set_skin(Some(group::header));
    for column in columns {
        let cell = cell(&header, column, Role::TableHeader);
        cell.text(column.title);
    }
    let widths: Vec<_> = columns.iter().map(|c| c.width).collect();
    let rows = table.list_view(row_height, rows, move |row, index| {
        let line = group::add(row, Role::TableRow, true);
        line.set_gap(0.0, 0.0);
        line.set_grow(1.0);
        for (column, &width) in widths.iter().enumerate() {
            let cell = cell(&line, &TableColumn { title: "", width }, Role::TableCell);
            fill(&cell, index, column)?;
        }
        Ok(())
    });
    rows.set_grow(1.0);
    // The table draws the border; its rows sit flush inside it.
    rows.set_border_width(0.0);
    rows.set_padding(0.0);
    Table {
        table: table.0,
        rows,
    }
}
