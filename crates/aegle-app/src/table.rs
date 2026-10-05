//! A header row over a virtual list of fixed-height rows.

use std::ops::Deref;

use aegle_core::Dirty;

use crate::{Container, ListView, Node, Result, Style, UiError, state::Semantic};

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
/// It dereferences to its outer column; [`Table::rows`] scrolls and resizes.
#[derive(Clone)]
pub struct Table {
    table: Container,
    rows: ListView,
}

impl Deref for Table {
    type Target = Container;
    fn deref(&self) -> &Container {
        &self.table
    }
}

impl Table {
    /// The virtual row list: `set_count`, `reload`, scrolling and its size.
    pub fn rows(&self) -> &ListView {
        &self.rows
    }
}

fn mark(node: &Node, semantic: Semantic) -> Result {
    node.change(|state, id| {
        state.tree.get_mut(id).unwrap().context.semantic = semantic;
        state.tree.mark_dirty(id, Dirty::SEMANTICS)?;
        Ok(())
    })
}

/// A header or body cell sized by its column.
fn cell(line: &Container, column: &TableColumn, padding: f32) -> Result<Container> {
    let cell = line.column()?;
    cell.set_padding(padding)?;
    match column.width {
        Some(width) => cell.set_width(Some(width))?,
        None => cell.set_grow(1.0)?,
    }
    Ok(cell)
}

impl Container {
    /// Appends a table of `rows` rows of `row_height`, with at least one column.
    /// `fill(cell, row, column)` fills an empty cell column when its row
    /// becomes visible; it runs outside the UI borrow, like
    /// [`Container::list_view`] rows. Give the table a height or flex space.
    pub fn table(
        &self,
        columns: &[TableColumn],
        row_height: f32,
        rows: usize,
        mut fill: impl FnMut(&Container, usize, usize) -> Result + 'static,
    ) -> Result<Table> {
        let valid = |width: Option<f32>| width.is_none_or(|w| w.is_finite() && w >= 0.0);
        if columns.is_empty() || !columns.iter().all(|c| valid(c.width)) {
            return Err(UiError::InvalidValue.into());
        }
        let theme = self.theme()?;
        let padding = theme.padding / 2.0;
        let table = self.column()?;
        table.set_gap(0.0)?;
        // Keeps the header fill inside the border.
        table.set_padding(1.0)?;
        table.set_style(Style {
            background: Some(theme.surface),
            border_color: Some(theme.border),
            border_width: Some(1.0),
            ..Default::default()
        })?;
        mark(&table, Semantic::Table)?;
        let header = table.row()?;
        header.set_gap(0.0)?;
        header.set_background(theme.background)?;
        mark(&header, Semantic::TableRow)?;
        for column in columns {
            let cell = cell(&header, column, padding)?;
            mark(&cell, Semantic::TableHeader)?;
            cell.text(column.title)?;
        }
        let widths: Vec<_> = columns.iter().map(|c| c.width).collect();
        let rows = table.list_view(row_height, rows, move |row, index| {
            let line = row.row()?;
            line.set_gap(0.0)?;
            line.set_grow(1.0)?;
            mark(&line, Semantic::TableRow)?;
            for (column, &width) in widths.iter().enumerate() {
                let cell = cell(&line, &TableColumn { title: "", width }, padding)?;
                mark(&cell, Semantic::TableCell)?;
                fill(&cell, index, column)?;
            }
            Ok(())
        })?;
        rows.set_grow(1.0)?;
        Ok(Table { table, rows })
    }
}
