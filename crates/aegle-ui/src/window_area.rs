//! Window areas: controls that move or resize the window when pressed.
//!
//! A custom title bar marks its container [`WindowArea::Move`]; the host
//! hands the resulting rectangles to the window platform, which begins the
//! native move or resize itself, so such a press never reaches the controls.
//! The rectangles follow paint order, later above earlier: a marked node
//! adds its visible bounds, and a usable interactive control lying over a
//! move or resize rectangle adds its own as [`WindowArea::Client`], so
//! buttons in a title bar keep their presses without being marked.
use aegle_core::NodeId;
use aegle_types::{Rect, WindowArea};

use crate::{Node, Ui, state::State};

/// Marked nodes and whether their rectangles may have changed.
#[derive(Default)]
pub(crate) struct WindowAreas {
    /// The area each marked node gives its bounds.
    marked: std::collections::HashMap<NodeId, WindowArea>,
    /// Layout, visibility or the marks changed since the host took the areas.
    stale: bool,
}

impl WindowAreas {
    pub(crate) fn forget(&mut self, id: NodeId) {
        self.stale |= self.marked.remove(&id).is_some();
    }

    /// Pixels changed, so marked nodes or the controls over them may have moved.
    pub(crate) fn repainted(&mut self) {
        self.stale |= !self.marked.is_empty();
    }
}

impl State {
    /// Appends the window's areas to `out` in paint order.
    fn collect_window_areas(&self, out: &mut Vec<(Rect, WindowArea)>) {
        for &id in &self.order {
            let element = &self.tree.get(id).unwrap().context;
            let shown = element.xf.map_or(element.bounds, |xf| {
                crate::scroll::map_rect(xf, element.bounds)
            });
            let visible = match element.clip {
                Some(clip) => clip.intersection(shown),
                None => (!shown.is_empty()).then_some(shown),
            };
            let Some(rect) = visible.filter(|_| element.effective_visible) else {
                continue;
            };
            let covers = |(other, area): &(Rect, WindowArea)| {
                *area != WindowArea::Client && other.intersection(rect).is_some()
            };
            let area = match self.window_areas.marked.get(&id) {
                Some(area) => *area,
                // A usable control keeps its presses where it lies over an area.
                None if element.control.interactive()
                    && out.iter().any(covers)
                    && self.usable(id) =>
                {
                    WindowArea::Client
                }
                None => continue,
            };
            out.push((rect, area));
        }
    }
}

impl Ui {
    /// Replaces `out` with the rectangles of the window that move or resize
    /// it and returns true, if they may have changed since they were last
    /// taken; otherwise leaves `out` alone and returns false. Rectangles are
    /// in logical window coordinates and paint order, a later one above an
    /// earlier one. Hosts call this after [`Self::refresh`] and hand a
    /// changed list to the window system; see [`Node::set_window_area`].
    pub fn take_window_areas(&self, out: &mut Vec<(Rect, WindowArea)>) -> bool {
        let mut state = self.write();
        if !std::mem::take(&mut state.window_areas.stale) {
            return false;
        }
        // Removed controls must not linger in the paint order.
        state.rebuild_order();
        out.clear();
        state.collect_window_areas(out);
        true
    }
}

impl Node {
    /// Makes pressing this control move ([`WindowArea::Move`]) or resize
    /// ([`WindowArea::Resize`]) its window, as a title bar or a size grip
    /// does on a window without the system's decorations; `None` removes the
    /// mark. The window system runs the move or resize, so the press and the
    /// drag are not delivered to controls. The area is the control's visible
    /// bounds, except where a usable interactive control such as a button
    /// lies over it; [`WindowArea::Client`] excludes any other control's
    /// bounds the same way. Resize areas act only while the window is
    /// resizable and neither maximized nor full screen. Hosts without a
    /// window to move, and windows that are layers or popups, ignore areas.
    pub fn set_window_area(&self, area: impl Into<Option<WindowArea>>) {
        let area = area.into();
        self.change(|state, id| {
            let areas = &mut state.window_areas;
            let old = match area {
                Some(area) => areas.marked.insert(id, area),
                None => areas.marked.remove(&id),
            };
            areas.stale |= old != area;
            Ok(())
        })
    }

    /// The area set on this control itself, if any.
    pub fn window_area(&self) -> Option<WindowArea> {
        self.change(|state, id| Ok(state.window_areas.marked.get(&id).copied()))
    }
}
