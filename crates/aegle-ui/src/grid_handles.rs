//! Grid and stack containers and grid item placement (the `grid` feature).

use aegle_layout::{Align, Display, Flow, GridLines, LayoutStyle, Placement, TemplateItem, Track};
use aegle_theme::{ControlKind, Theme};

use aegle_core::NodeId;

use crate::{
    Container, Node, State,
    control::{Control, Plain},
    ui::container_style,
};

/// A container whose children overlap in its single cell.
pub struct Stack;

impl Control for Stack {
    fn kind(&self) -> &'static ControlKind {
        &crate::control::CONTAINER
    }
    fn retheme(
        &self,
        theme: &Theme,
        local: crate::LocalLayout,
        root: bool,
        style: &mut LayoutStyle,
    ) {
        Plain.retheme(theme, local, root, style);
    }
}

/// Places a new or moved child of a stack, directly or through contents
/// groups, in the stack's single cell.
pub(crate) fn stack_child(state: &State, mut parent: NodeId, style: &mut LayoutStyle) {
    while state.tree.get(parent).unwrap().is_contents() {
        parent = state.tree.parent(parent).unwrap().unwrap();
    }
    let control = &state.tree.get(parent).unwrap().context.control;
    if (&**control as &dyn std::any::Any).is::<Stack>() {
        style
            .set_grid_row(Placement::at(1))
            .set_grid_column(Placement::at(1));
    }
}

impl Container {
    /// Appends a grid, like QML's `GridLayout`; give it columns with
    /// [`Self::set_columns`]. Children fill the cells row by row, adding
    /// automatic rows as needed; [`Node::set_grid_column`] and
    /// [`Node::set_grid_row`] place them explicitly.
    pub fn grid(&self) -> Container {
        Container(self.add(|_, theme| {
            let mut style = container_style(theme, false);
            style.set_display(Display::Grid);
            Ok((Box::new(Plain), style))
        }))
    }
    /// Appends a stack: every child overlaps in one cell, which takes the
    /// available space and is at least as large as its largest child. Children
    /// stretch by default; `set_align_self` and `set_justify_self` place them,
    /// and the last child paints over the others.
    pub fn stack(&self) -> Container {
        Container(self.add(|_, theme| {
            let mut style = container_style(theme, false);
            style
                .set_display(Display::Grid)
                .set_columns(&[Track::Fr(1.0)])
                .set_rows(&[Track::Fr(1.0)]);
            Ok((Box::new(Stack), style))
        }))
    }
    /// Replaces the explicit columns, like CSS `grid-template-columns`: plain
    /// [`Track`]s, or [`TemplateItem`]s that also name lines and repeat tracks.
    pub fn set_columns<T: Clone + Into<TemplateItem>>(&self, columns: &[T]) {
        self.layout(crate::LocalLayout::NONE, |s| s.set_columns(columns))
    }
    /// Replaces the explicit rows like [`Self::set_columns`]; rows beyond them
    /// use [`Self::set_auto_rows`].
    pub fn set_rows<T: Clone + Into<TemplateItem>>(&self, rows: &[T]) {
        self.layout(crate::LocalLayout::NONE, |s| s.set_rows(rows))
    }
    /// Names rectangular areas, like CSS `grid-template-areas`: one string per
    /// row of whitespace-separated cell names, `.` for an unnamed cell. The
    /// areas extend the explicit grid with automatic tracks where it has fewer.
    /// Children enter an area with [`Node::set_grid_area`]; an empty slice clears.
    pub fn set_areas(&self, rows: &[&str]) {
        self.layout(crate::LocalLayout::NONE, |s| s.set_areas(rows))
    }
    /// Sizes rows the grid adds beyond its explicit tracks, cycling through
    /// `rows`; empty restores automatic rows.
    pub fn set_auto_rows(&self, rows: &[Track]) {
        self.layout(crate::LocalLayout::NONE, |s| s.set_auto_rows(rows))
    }
    /// Sizes columns the grid adds beyond its explicit tracks.
    pub fn set_auto_columns(&self, columns: &[Track]) {
        self.layout(crate::LocalLayout::NONE, |s| s.set_auto_columns(columns))
    }
    /// Sets the order in which the grid places children without a position.
    pub fn set_flow(&self, flow: Flow) {
        self.layout(crate::LocalLayout::NONE, |s| s.set_flow(flow))
    }
    /// Aligns children horizontally within their grid areas; `None` stretches.
    pub fn set_justify_items(&self, align: impl Into<Option<Align>>) {
        let align = align.into();
        self.layout(crate::LocalLayout::NONE, |s| s.set_justify_items(align))
    }
}

impl Node {
    /// Places this grid child on the column axis: a [`Placement`] by number,
    /// or [`GridLines`] that may use line and area names.
    pub fn set_grid_column(&self, placement: impl Into<GridLines>) {
        let lines = placement.into();
        self.layout(crate::LocalLayout::NONE, |s| s.set_grid_column(lines))
    }
    /// Places this grid child on the row axis.
    pub fn set_grid_row(&self, placement: impl Into<GridLines>) {
        let lines = placement.into();
        self.layout(crate::LocalLayout::NONE, |s| s.set_grid_row(lines))
    }
    /// Places this grid child in the area called `name` on both axes, like
    /// CSS `grid-area: name`; see [`Container::set_areas`].
    pub fn set_grid_area(&self, name: &str) {
        let lines = GridLines::named(name);
        self.layout(crate::LocalLayout::NONE, |s| {
            s.set_grid_row(lines.clone()).set_grid_column(lines)
        })
    }
    /// Overrides the grid's horizontal alignment for this child; `None` follows the grid.
    pub fn set_justify_self(&self, align: impl Into<Option<Align>>) {
        let align = align.into();
        self.layout(crate::LocalLayout::NONE, |s| s.set_justify_self(align))
    }
}
