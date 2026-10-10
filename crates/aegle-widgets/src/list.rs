use std::{ops::Deref, rc::Rc};

use aegle_core::NodeId;
use aegle_layout::{Dimension, Edges, LengthPercentageAuto, Overflow, Position, Style};
use aegle_theme::Theme;
use aegle_types::Rect;
use aegle_ui::{
    Container, Node, Plain, Result, State, Ui, UiError, container_style,
    scroll_geometry::intersection,
};

use crate::{ScrollView, Widgets};

/// Builds the content of one row from its index.
type RowBuilder = Box<dyn FnMut(&Container, usize) -> Result>;

/// Virtual lists of one UI, kept in the engine's per-library storage.
#[derive(Default)]
pub(crate) struct Lists {
    /// Virtual list viewports and their realized rows.
    pub items: Vec<(NodeId, List)>,
}

fn lists(state: &mut State) -> &mut Lists {
    state.ext()
}

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
    builder: Option<RowBuilder>,
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
pub struct ListView(pub ScrollView);

impl Deref for ListView {
    type Target = ScrollView;
    fn deref(&self) -> &ScrollView {
        &self.0
    }
}

/// See [`Widgets::list_view`] and [`Widgets::variable_list_view`].
pub(crate) fn virtual_list(
    container: &Container,
    row_height: f32,
    count: usize,
    variable: bool,
    row: RowBuilder,
) -> ListView {
    let height = extent(row_height, count);
    let view = container.scroll_view();
    view.change(|state, id| {
        state.install(&crate::HOOKS);
        let mut style = state.tree.get(id).unwrap().style().clone();
        style.flex_shrink = 1.0;
        aegle_layout::set_style(&mut state.tree, id, style)?;
        let mut style = container_style(state.theme_of(id), false);
        style.size.height = Dimension::length(height);
        style.flex_shrink = 0.0;
        let spacer = state.insert(id, usize::MAX, Box::new(Plain), style)?;
        lists(state).items.push((
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
    });
    ListView(view)
}

impl ListView {
    fn list<T>(&self, change: impl FnOnce(&mut State, &mut List) -> Result<T>) -> T {
        self.change(|state, id| {
            let (index, mut list) = take(state, id);
            let result = change(state, &mut list);
            put_back(state, index, id, list);
            result
        })
    }
    /// Returns the number of rows.
    pub fn count(&self) -> usize {
        self.list(|_, list| Ok(list.count))
    }
    /// Returns the fixed logical row height, or the estimate of a list whose
    /// rows size to their content.
    pub fn row_height(&self) -> f32 {
        self.list(|_, list| Ok(list.row_height))
    }
    /// Changes the row count. Realized rows at or past it are removed on refresh;
    /// rows below it keep their controls.
    pub fn set_count(&self, count: usize) {
        self.list(|state, list| {
            let mut height = extent(list.row_height, count);
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
    pub fn reload(&self) {
        self.list(|state, list| {
            list.reload = true;
            state.repaint = true;
            Ok(())
        })
    }
}

fn extent(row_height: f32, count: usize) -> f32 {
    let height = row_height * count as f32;
    aegle_ui::require(row_height.is_finite() && row_height > 0.0 && height <= MAX_EXTENT);
    height
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

/// Takes a list out of storage, remembering where it was.
fn take(state: &mut State, id: NodeId) -> (usize, List) {
    let index = lists(state)
        .items
        .iter()
        .position(|(list, _)| *list == id)
        .unwrap();
    (index, lists(state).items.remove(index).1)
}

/// Returns a taken list to its place, or the end if lists before it were removed.
fn put_back(state: &mut State, index: usize, id: NodeId, list: List) {
    let items = &mut lists(state).items;
    items.insert(index.min(items.len()), (id, list));
}

fn plan_rows(state: &mut State, id: NodeId) -> Result<(Vec<(usize, NodeId)>, bool)> {
    // The list is taken out while its rows change, so row removal can drop
    // nested lists without touching it.
    let index = lists(state)
        .items
        .iter()
        .position(|(list, _)| *list == id)
        .unwrap();
    let (_, mut list) = lists(state).items.swap_remove(index);
    let result = plan(state, id, &mut list);
    lists(state).items.push((id, list));
    result
}

/// Removes rows outside the visible range and inserts empty rows entering it.
/// Returns the new rows and whether any row was removed.
fn plan(state: &mut State, id: NodeId, list: &mut List) -> Result<(Vec<(usize, NodeId)>, bool)> {
    let (row_height, spacer, reload) = (list.row_height, list.spacer, list.reload);
    list.reload = false;
    let mut rows = std::mem::take(&mut list.rows);
    let view = &state.tree.get(id).unwrap().context;
    let top = state.tree.get(spacer).unwrap().context.bounds.origin.y;
    let window = Rect::new(0.0, 0.0, state.size.width, state.size.height);
    let visible = intersection(
        view.clip
            .map_or(view.bounds, |clip| intersection(clip, view.bounds)),
        window,
    );
    let count = list.count;
    let (start, end) = (
        visible.origin.y - top,
        visible.origin.y + visible.size.height - top,
    );
    let range = if !view.effective_visible || visible.is_empty() {
        0..0
    } else if let Some(offsets) = &list.offsets {
        let first = offsets.partition_point(|&o| o <= start).saturating_sub(1);
        first.min(count)..offsets.partition_point(|&o| o < end).min(count)
    } else {
        let first = (start / row_height).floor().max(0.0) as usize;
        first.min(count)..((end / row_height).ceil().max(0.0) as usize).min(count)
    };
    // Placements are read now: removing rows below may remove nested lists.
    let placements: Vec<_> = range
        .clone()
        .map(|row| match &list.offsets {
            Some(offsets) => (offsets[row], None),
            None => (row as f32 * row_height, Some(row_height)),
        })
        .collect();
    let mut removed = false;
    let mut i = 0;
    while i < rows.len() {
        let (row, node) = rows[i];
        let alive = state.tree.get(node).is_some();
        if alive && !reload && range.contains(&row) {
            i += 1;
            continue;
        }
        rows.remove(i);
        removed = true;
        if alive {
            state.remove_subtree(node)?;
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
        let style = row_style(state.theme_of(spacer), top, height);
        let node = state.insert(spacer, position, Box::new(Plain), style)?;
        rows.insert(position, (row, node));
        created.push((row, node));
    }
    list.rows = rows;
    Ok((created, removed))
}

/// Forgets lists of removed nodes.
pub(crate) fn removed(state: &mut State, node: NodeId) {
    if state.ext_ref::<Lists>().is_some() {
        lists(state).items.retain(|(list, _)| *list != node);
    }
}

/// Measures the realized rows of content-sized lists after layout and moves
/// the rows after them. Returns whether any row moved.
pub(crate) fn measure_rows(state: &mut State) -> Result<bool> {
    if state.ext_ref::<Lists>().is_none() {
        return Ok(false);
    }
    let mut changed = false;
    for index in 0..lists(state).items.len() {
        let (rows, variable) = {
            let list = &lists(state).items[index].1;
            (list.rows.clone(), list.offsets.is_some())
        };
        if !variable {
            continue;
        }
        let heights: Vec<_> = rows
            .iter()
            .map(|&(row, node)| (row, state.tree.get(node).unwrap().bounds().size.height))
            .collect();
        let list = &mut lists(state).items[index].1;
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
        let mut style = state.tree.get(spacer).unwrap().style().clone();
        style.size.height = Dimension::length(total);
        aegle_layout::set_style(&mut state.tree, spacer, style)?;
        for (node, top) in tops {
            let mut style = state.tree.get(node).unwrap().style().clone();
            style.inset.top = LengthPercentageAuto::length(top);
            aegle_layout::set_style(&mut state.tree, node, style)?;
        }
    }
    Ok(changed)
}

/// Realizes rows entering list viewports and removes rows leaving them.
/// Row callbacks run outside the state borrow. Returns whether rows changed.
pub(crate) fn realize_rows(ui: &Ui) -> Result<bool> {
    let mut changed = false;
    let mut index = 0;
    loop {
        let (id, created, builder) = {
            let mut state = ui
                .state
                .try_borrow_mut()
                .map_err(|_| UiError::ReentrantAccess)?;
            let Some(&(id, _)) = state.ext_ref::<Lists>().and_then(|l| l.items.get(index)) else {
                break;
            };
            index += 1;
            let (created, removed) = plan_rows(&mut state, id)?;
            changed |= removed || !created.is_empty();
            if created.is_empty() {
                continue;
            }
            let list = lists(&mut state)
                .items
                .iter_mut()
                .find(|(list, _)| *list == id);
            (id, created, list.unwrap().1.builder.take())
        };
        let Some(mut builder) = builder else {
            continue;
        };
        let mut result = Ok(());
        for (row, node) in created {
            let container = Container(Node {
                state: Rc::downgrade(&ui.state),
                id: node,
            });
            if container.is_alive() {
                result = builder(&container, row);
            }
            if result.is_err() {
                break;
            }
        }
        let mut state = ui.state.borrow_mut();
        if let Some((_, list)) = lists(&mut state)
            .items
            .iter_mut()
            .find(|(list, _)| *list == id)
        {
            list.builder = Some(builder);
        }
        result?;
    }
    Ok(changed)
}
