use std::{ops::Deref, rc::Rc};

use aegle_core::NodeId;
use aegle_layout::{Dimension, Edges, LengthPercentageAuto, Overflow, Position, Style};
use aegle_theme::Theme;
use aegle_types::Rect;
use aegle_widgets::intersection;

use crate::{
    Container, Node, Result, ScrollView, Ui, UiError,
    state::{Content, State},
    ui::container_style,
};

/// Largest `count × row_height` for which f32 layout keeps whole-pixel positions.
const MAX_EXTENT: f32 = 16_777_216.0;

pub(crate) struct List {
    /// The fixed height, or the estimate for rows not yet measured.
    row_height: f32,
    count: usize,
    /// Row tops plus the total height (`count + 1` entries) when rows size to
    /// their content; `None` for equal heights.
    offsets: Option<Vec<f32>>,
    spacer: NodeId,
    /// Realized rows sorted by index; they are the spacer's only children.
    rows: Vec<(usize, NodeId)>,
    builder: Option<Box<dyn FnMut(&Container, usize) -> Result>>,
    reload: bool,
}

/// A vertical list of equal-height or content-sized rows over a scroll viewport.
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
        self.virtual_list(row_height, count, false, Box::new(row))
    }

    /// Appends a virtual list whose rows size to their content. Rows not yet
    /// shown count as `estimate` high; shown rows are measured after layout and
    /// later rows move accordingly. Scrolling back may shift content while
    /// estimates are replaced. `count × estimate` must not exceed 16,777,216.
    pub fn variable_list_view(
        &self,
        estimate: f32,
        count: usize,
        row: impl FnMut(&Container, usize) -> Result + 'static,
    ) -> Result<ListView> {
        self.virtual_list(estimate, count, true, Box::new(row))
    }

    fn virtual_list(
        &self,
        row_height: f32,
        count: usize,
        variable: bool,
        row: Box<dyn FnMut(&Container, usize) -> Result>,
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
                    offsets: variable.then(|| (0..=count).map(|i| i as f32 * row_height).collect()),
                    spacer,
                    rows: Vec::new(),
                    builder: Some(row),
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
    /// Returns the fixed logical row height, or the estimate of a list whose
    /// rows size to their content.
    pub fn row_height(&self) -> Result<f32> {
        self.list(|_, list| Ok(list.row_height))
    }
    /// Changes the row count. Realized rows at or past it are removed on refresh;
    /// rows below it keep their controls.
    pub fn set_count(&self, count: usize) -> Result {
        self.list(|state, list| {
            let mut height = extent(list.row_height, count)?;
            if let Some(offsets) = &mut list.offsets {
                offsets.truncate(count.min(list.count) + 1);
                while offsets.len() <= count {
                    offsets.push(offsets.last().unwrap() + list.row_height);
                }
                height = offsets[count];
            }
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

/// A row at `top`, of a fixed height or sized to its content.
fn row_style(theme: &Theme, top: f32, height: Option<f32>) -> Style {
    let mut style = container_style(theme, false);
    style.position = Position::Absolute;
    style.inset = Edges {
        left: LengthPercentageAuto::length(0.0),
        right: LengthPercentageAuto::length(0.0),
        top: LengthPercentageAuto::length(top),
        bottom: LengthPercentageAuto::auto(),
    };
    style.size.height = height.map_or(Dimension::auto(), Dimension::length);
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
        let (start, end) = (
            visible.origin.y - top,
            visible.origin.y + visible.size.height - top,
        );
        let range = if !view.effective_visible || visible.is_empty() {
            0..0
        } else if let Some(offsets) = &self.lists[index].1.offsets {
            let first = offsets.partition_point(|&o| o <= start).saturating_sub(1);
            first.min(count)..offsets.partition_point(|&o| o < end).min(count)
        } else {
            let first = (start / row_height).floor().max(0.0) as usize;
            first.min(count)..((end / row_height).ceil().max(0.0) as usize).min(count)
        };
        // Placements are read now: removing rows below may remove nested lists.
        let placements: Vec<_> = range
            .clone()
            .map(|row| match &self.lists[index].1.offsets {
                Some(offsets) => (offsets[row], None),
                None => (row as f32 * row_height, Some(row_height)),
            })
            .collect();
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
        let placements_start = range.start;
        for row in range {
            let position = rows.partition_point(|&(r, _)| r < row);
            if rows.get(position).is_some_and(|&(r, _)| r == row) {
                continue;
            }
            let (top, height) = placements[row - placements_start];
            let style = row_style(self.theme_of(spacer), top, height);
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

impl State {
    /// Measures the realized rows of content-sized lists after layout and moves
    /// the rows after them. Returns whether any row moved.
    pub fn measure_rows(&mut self) -> Result<bool> {
        let mut changed = false;
        for index in 0..self.lists.len() {
            let list = &self.lists[index].1;
            if list.offsets.is_none() {
                continue;
            }
            let heights: Vec<_> = list
                .rows
                .iter()
                .map(|&(row, node)| (row, self.tree.get(node).unwrap().bounds().size.height))
                .collect();
            let list = &mut self.lists[index].1;
            let offsets = list.offsets.as_mut().unwrap();
            let mut moved = false;
            for (row, height) in heights {
                let delta = height - (offsets[row + 1] - offsets[row]);
                if delta.abs() > 0.001 {
                    offsets[row + 1..]
                        .iter_mut()
                        .for_each(|offset| *offset += delta);
                    moved = true;
                }
            }
            if !moved {
                continue;
            }
            changed = true;
            let total = offsets[list.count];
            if total > MAX_EXTENT {
                return Err(UiError::InvalidValue.into());
            }
            let tops: Vec<_> = list
                .rows
                .iter()
                .map(|&(row, node)| (node, offsets[row]))
                .collect();
            let spacer = list.spacer;
            let mut style = self.tree.get(spacer).unwrap().style().clone();
            style.size.height = Dimension::length(total);
            aegle_layout::set_style(&mut self.tree, spacer, style)?;
            for (node, top) in tops {
                let mut style = self.tree.get(node).unwrap().style().clone();
                style.inset.top = LengthPercentageAuto::length(top);
                aegle_layout::set_style(&mut self.tree, node, style)?;
            }
        }
        Ok(changed)
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
