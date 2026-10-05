use std::{ops::Deref, rc::Rc};

use aegle_core::NodeId;
use aegle_layout::{Dimension, Edges, LengthPercentageAuto, Overflow, Position, Style};
use aegle_theme::Theme;
use aegle_types::Rect;

use crate::{
    Container, Node, Result, ScrollView, Ui, UiError,
    scroll::intersection,
    state::{Content, State},
    ui::container_style,
};

/// Largest `count × row_height` for which f32 layout keeps whole-pixel positions.
const MAX_EXTENT: f32 = 16_777_216.0;

pub(crate) struct List {
    row_height: f32,
    count: usize,
    spacer: NodeId,
    /// Realized rows sorted by index; they are the spacer's only children.
    rows: Vec<(usize, NodeId)>,
    builder: Option<Box<dyn FnMut(&Container, usize) -> Result>>,
    reload: bool,
}

/// A vertical list of equal-height rows over a scroll viewport.
///
/// Only rows intersecting the visible viewport (also clipped by ancestor views
/// and the window) exist as controls. A row leaving that range is removed with
/// its focus and local state; a row entering it is rebuilt by the row callback.
/// The list shrinks to its parent's space by default; give it a height or a
/// constrained flex allocation.
#[derive(Clone)]
pub struct ListView(ScrollView);

impl Deref for ListView {
    type Target = ScrollView;
    fn deref(&self) -> &ScrollView {
        &self.0
    }
}

impl Container {
    /// Appends a virtual list of `count` rows of `row_height` logical pixels.
    /// `row` fills an empty row column for an index. It runs during [`Ui::refresh`]
    /// outside the UI borrow, so it may use any handle. `count × row_height`
    /// must not exceed 16,777,216.
    pub fn list_view(
        &self,
        row_height: f32,
        count: usize,
        row: impl FnMut(&Container, usize) -> Result + 'static,
    ) -> Result<ListView> {
        let height = extent(row_height, count)?;
        let view = self.scroll_view()?;
        view.change(|state, id| {
            let mut style = state.tree.get(id).unwrap().style().clone();
            style.flex_shrink = 1.0;
            aegle_layout::set_style(&mut state.tree, id, style)?;
            let mut style = container_style(state.theme_of(id), false);
            style.size.height = Dimension::length(height);
            style.flex_shrink = 0.0;
            let spacer = state.insert(id, usize::MAX, Content::Container, style)?;
            state.lists.push((
                id,
                List {
                    row_height,
                    count,
                    spacer,
                    rows: Vec::new(),
                    builder: Some(Box::new(row)),
                    reload: false,
                },
            ));
            Ok(())
        })?;
        Ok(ListView(view))
    }
}

impl ListView {
    fn list<T>(&self, change: impl FnOnce(&mut State, &mut List) -> Result<T>) -> Result<T> {
        self.change(|state, id| {
            let index = state.lists.iter().position(|(list, _)| *list == id);
            let (_, mut list) = state.lists.swap_remove(index.unwrap());
            let result = change(state, &mut list);
            state.lists.push((id, list));
            result
        })
    }
    /// Returns the number of rows.
    pub fn count(&self) -> Result<usize> {
        self.list(|_, list| Ok(list.count))
    }
    /// Returns the fixed logical row height.
    pub fn row_height(&self) -> Result<f32> {
        self.list(|_, list| Ok(list.row_height))
    }
    /// Changes the row count. Realized rows at or past it are removed on refresh;
    /// rows below it keep their controls.
    pub fn set_count(&self, count: usize) -> Result {
        self.list(|state, list| {
            let height = extent(list.row_height, count)?;
            list.count = count;
            let mut style = state.tree.get(list.spacer).unwrap().style().clone();
            style.size.height = Dimension::length(height);
            aegle_layout::set_style(&mut state.tree, list.spacer, style)?;
            state.repaint = true;
            Ok(())
        })
    }
    /// Rebuilds every realized row on the next refresh, for changed row data.
    pub fn reload(&self) -> Result {
        self.list(|state, list| {
            list.reload = true;
            state.repaint = true;
            Ok(())
        })
    }
}

fn extent(row_height: f32, count: usize) -> Result<f32> {
    let height = row_height * count as f32;
    if row_height.is_finite() && row_height > 0.0 && height <= MAX_EXTENT {
        Ok(height)
    } else {
        Err(UiError::InvalidValue.into())
    }
}

fn row_style(theme: &Theme, index: usize, row_height: f32) -> Style {
    let mut style = container_style(theme, false);
    style.position = Position::Absolute;
    style.inset = Edges {
        left: LengthPercentageAuto::length(0.0),
        right: LengthPercentageAuto::length(0.0),
        top: LengthPercentageAuto::length(index as f32 * row_height),
        bottom: LengthPercentageAuto::auto(),
    };
    style.size.height = Dimension::length(row_height);
    // Row contents never widen the list's scrollable overflow.
    style.overflow.x = Overflow::Hidden;
    style.overflow.y = Overflow::Hidden;
    style
}

impl State {
    /// Removes rows outside the visible range and inserts empty rows entering it.
    /// Returns the new rows and whether any row was removed.
    fn plan_rows(&mut self, id: NodeId) -> Result<(Vec<(usize, NodeId)>, bool)> {
        let index = self.lists.iter().position(|(list, _)| *list == id).unwrap();
        let list = &mut self.lists[index].1;
        let (row_height, spacer, reload) = (list.row_height, list.spacer, list.reload);
        list.reload = false;
        let mut rows = std::mem::take(&mut list.rows);
        let view = &self.tree.get(id).unwrap().context;
        let top = self.tree.get(spacer).unwrap().context.bounds.origin.y;
        let window = Rect::new(0.0, 0.0, self.size.width, self.size.height);
        let visible = intersection(
            view.clip
                .map_or(view.bounds, |clip| intersection(clip, view.bounds)),
            window,
        );
        let count = self.lists[index].1.count;
        let range = if view.effective_visible && !visible.is_empty() {
            let first = ((visible.origin.y - top) / row_height).floor().max(0.0) as usize;
            let end = ((visible.origin.y + visible.size.height - top) / row_height).ceil();
            first.min(count)..(end.max(0.0) as usize).min(count)
        } else {
            0..0
        };
        let mut removed = false;
        let mut i = 0;
        while i < rows.len() {
            let (row, node) = rows[i];
            let alive = self.tree.get(node).is_some();
            if alive && !reload && range.contains(&row) {
                i += 1;
                continue;
            }
            rows.remove(i);
            removed = true;
            if alive {
                self.remove_subtree(node)?;
            }
        }
        let mut created = Vec::new();
        for row in range {
            let position = rows.partition_point(|&(r, _)| r < row);
            if rows.get(position).is_some_and(|&(r, _)| r == row) {
                continue;
            }
            let style = row_style(self.theme_of(spacer), row, row_height);
            let node = self.insert(spacer, position, Content::Container, style)?;
            rows.insert(position, (row, node));
            created.push((row, node));
        }
        // Removing a row may have removed nested lists, so look this one up again.
        if let Some((_, list)) = self.lists.iter_mut().find(|(list, _)| *list == id) {
            list.rows = rows;
        }
        Ok((created, removed))
    }
}

impl Ui {
    /// Realizes rows entering list viewports and removes rows leaving them.
    /// Row callbacks run outside the state borrow. Returns whether rows changed.
    pub(crate) fn realize_rows(&self) -> Result<bool> {
        let mut changed = false;
        let mut index = 0;
        loop {
            let (id, created, builder) = {
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .map_err(|_| UiError::ReentrantAccess)?;
                let Some(&(id, _)) = state.lists.get(index) else {
                    break;
                };
                index += 1;
                let (created, removed) = state.plan_rows(id)?;
                changed |= removed || !created.is_empty();
                if created.is_empty() {
                    continue;
                }
                let list = state.lists.iter_mut().find(|(list, _)| *list == id);
                (id, created, list.unwrap().1.builder.take())
            };
            let Some(mut builder) = builder else {
                continue;
            };
            let mut result = Ok(());
            for (row, node) in created {
                let container = Container(Node {
                    state: Rc::downgrade(&self.state),
                    id: node,
                });
                if container.is_alive() {
                    result = builder(&container, row);
                    if result.is_err() {
                        break;
                    }
                }
            }
            let mut state = self.state.borrow_mut();
            if let Some((_, list)) = state.lists.iter_mut().find(|(list, _)| *list == id) {
                list.builder = Some(builder);
            }
            result?;
        }
        Ok(changed)
    }
}
