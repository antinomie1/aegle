//! Grid and stack containers and grid item placement (the `grid` feature).

use std::any::Any;

use aegle_layout::{Align, Display, Flow, GridLines, Placement, Style, TemplateItem, Track};
use aegle_theme::{ControlKind, Theme};

use aegle_core::NodeId;

use crate::{
    Container, Node, Result, State, UiError,
    control::{Control, Plain},
    ui::container_style,
};

/// A container whose children overlap in its single cell.
pub struct Stack;

impl Control for Stack {
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
        Plain.retheme(theme, local, root, style);
    }
}

/// Places a new or moved child of a stack, directly or through contents
/// groups, in the stack's single cell.
pub(crate) fn stack_child(state: &State, mut parent: NodeId, style: &mut Style) {
    while state.tree.get(parent).unwrap().is_contents() {
        parent = state.tree.parent(parent).unwrap().unwrap();
    }
    let control = &state.tree.get(parent).unwrap().context.control;
    if control.as_any().is::<Stack>() {
        style.grid_row = Placement::at(1).lines();
        style.grid_column = Placement::at(1).lines();
    }
}

fn tracks(tracks: &[Track]) -> Result<Vec<aegle_layout::GridTemplateComponent<String>>> {
    if !tracks.iter().all(|track| track.is_valid()) {
        return Err(UiError::InvalidValue.into());
    }
    Ok(tracks.iter().map(|track| track.template()).collect())
}

fn sizing(implicit: &[Track]) -> Result<Vec<aegle_layout::TrackSizingFunction>> {
    if !implicit.iter().all(|track| track.is_valid()) {
        return Err(UiError::InvalidValue.into());
    }
    Ok(implicit.iter().map(|track| track.sizing()).collect())
}

fn placement_lines(
    lines: GridLines,
) -> Result<aegle_layout::Line<aegle_layout::GridPlacement<String>>> {
    if !lines.is_valid() {
        return Err(UiError::InvalidValue.into());
    }
    Ok(lines.lines())
}

impl Container {
    /// Appends a grid with explicit column tracks, like QML's `GridLayout`.
    /// Children fill the cells row by row, adding automatic rows as needed;
    /// [`Node::set_grid_column`] and [`Node::set_grid_row`] place them explicitly.
    pub fn grid(&self, columns: &[Track]) -> Result<Container> {
        let columns = tracks(columns)?;
        self.add(|_, theme| {
            let mut style = container_style(theme, false);
            style.display = Display::Grid;
            style.grid_template_columns = columns;
            Ok((Box::new(Plain), style))
        })
        .map(Container)
    }
    /// Appends a stack: every child overlaps in one cell, which takes the
    /// available space and is at least as large as its largest child. Children
    /// stretch by default; `set_align_self` and `set_justify_self` place them,
    /// and the last child paints over the others.
    pub fn stack(&self) -> Result<Container> {
        self.add(|_, theme| {
            let mut style = container_style(theme, false);
            style.display = Display::Grid;
            style.grid_template_columns = vec![Track::Fr(1.0).template()];
            style.grid_template_rows = vec![Track::Fr(1.0).template()];
            Ok((Box::new(Stack), style))
        })
        .map(Container)
    }
    /// Replaces the explicit column tracks of a grid.
    pub fn set_columns(&self, columns: &[Track]) -> Result {
        let columns = tracks(columns)?;
        self.layout(0, |s| s.grid_template_columns = columns)
    }
    /// Replaces the explicit row tracks of a grid; rows beyond them use
    /// [`Self::set_auto_rows`].
    pub fn set_rows(&self, rows: &[Track]) -> Result {
        let rows = tracks(rows)?;
        self.layout(0, |s| s.grid_template_rows = rows)
    }
    /// Replaces the explicit columns with a track list that may name lines and
    /// repeat tracks, like CSS `grid-template-columns`; see [`TemplateItem`].
    pub fn set_column_template(&self, items: &[TemplateItem]) -> Result {
        let (columns, names) = aegle_layout::template(items).ok_or(UiError::InvalidValue)?;
        self.layout(0, |s| {
            s.grid_template_columns = columns;
            s.grid_template_column_names = names;
        })
    }
    /// Replaces the explicit rows with a track list, like CSS `grid-template-rows`.
    pub fn set_row_template(&self, items: &[TemplateItem]) -> Result {
        let (rows, names) = aegle_layout::template(items).ok_or(UiError::InvalidValue)?;
        self.layout(0, |s| {
            s.grid_template_rows = rows;
            s.grid_template_row_names = names;
        })
    }
    /// Names rectangular areas, like CSS `grid-template-areas`: one string per
    /// row of whitespace-separated cell names, `.` for an unnamed cell. The
    /// areas extend the explicit grid with automatic tracks where it has fewer.
    /// Children enter an area with [`Node::set_grid_area`]; an empty slice clears.
    pub fn set_areas(&self, rows: &[&str]) -> Result {
        let areas = match rows {
            [] => None,
            rows => Some(aegle_layout::areas(rows).ok_or(UiError::InvalidValue)?),
        };
        self.layout(0, |s| s.grid_template_areas = areas)
    }
    /// Sizes rows the grid adds beyond its explicit tracks, cycling through
    /// `rows`; empty restores automatic rows.
    pub fn set_auto_rows(&self, rows: &[Track]) -> Result {
        let rows = sizing(rows)?;
        self.layout(0, |s| s.grid_auto_rows = rows)
    }
    /// Sizes columns the grid adds beyond its explicit tracks.
    pub fn set_auto_columns(&self, columns: &[Track]) -> Result {
        let columns = sizing(columns)?;
        self.layout(0, |s| s.grid_auto_columns = columns)
    }
    /// Sets the order in which the grid places children without a position.
    pub fn set_flow(&self, flow: Flow) -> Result {
        self.layout(0, |s| s.grid_auto_flow = flow.auto_flow())
    }
    /// Aligns children horizontally within their grid areas; `None` stretches.
    pub fn set_justify_items(&self, align: Option<Align>) -> Result {
        self.layout(0, |s| s.justify_items = align.map(Align::items))
    }
}

impl Node {
    /// Places this grid child on the column axis: a [`Placement`] by number,
    /// or [`GridLines`] that may use line and area names.
    pub fn set_grid_column(&self, placement: impl Into<GridLines>) -> Result {
        let lines = placement_lines(placement.into())?;
        self.layout(0, |s| s.grid_column = lines)
    }
    /// Places this grid child on the row axis.
    pub fn set_grid_row(&self, placement: impl Into<GridLines>) -> Result {
        let lines = placement_lines(placement.into())?;
        self.layout(0, |s| s.grid_row = lines)
    }
    /// Places this grid child in the area called `name` on both axes, like
    /// CSS `grid-area: name`; see [`Container::set_areas`].
    pub fn set_grid_area(&self, name: &str) -> Result {
        let lines = placement_lines(GridLines::named(name))?;
        self.layout(0, |s| {
            s.grid_row = lines.clone();
            s.grid_column = lines;
        })
    }
    /// Overrides the grid's horizontal alignment for this child; `None` follows the grid.
    pub fn set_justify_self(&self, align: Option<Align>) -> Result {
        self.layout(0, |s| s.justify_self = align.map(Align::items))
    }
}
