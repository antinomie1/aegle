//! The window area whose pixels changed, so a host can redraw and present
//! only that part. Re-recorded, moved, resized or newly clipped nodes add
//! the old and new areas their records draw, even beyond the node's bounds;
//! structure changes, resizing the window and a new window background damage
//! all of it.

use crate::{Ui, state::State};
use aegle_core::NodeId;
use aegle_types::{Rect, Region};

impl State {
    /// Damages the old and new area of a node whose record changed or whose
    /// area moved, grew, shrank or was clipped since the last refresh.
    pub(crate) fn damage_node(&mut self, id: NodeId, recorded: bool) {
        let area = self.painted_area(id);
        let element = &mut self.tree.get_mut(id).unwrap().context;
        if !recorded && element.painted == area {
            return;
        }
        let old = std::mem::replace(&mut element.painted, area);
        // A small move adds one merged rectangle, a jump two.
        old.into_iter()
            .chain(area)
            .for_each(|rect| self.damage.add(rect));
    }

    /// The presented window area of a node's records: the bounds of what its
    /// scene and overlay draw, wherever that is relative to the node, placed
    /// and clipped as when drawn; `None` when it draws nothing visible.
    fn painted_area(&self, id: NodeId) -> Option<Rect> {
        let element = &self.tree.get(id).unwrap().context;
        if !element.effective_visible {
            return None;
        }
        let overlay = element
            .overlay
            .as_ref()
            .and_then(|overlay| overlay.bounds());
        let local = match (element.scene.bounds(), overlay) {
            (Some(scene), Some(overlay)) => scene.union(overlay),
            (scene, overlay) => scene.or(overlay)?,
        };
        let origin = element.bounds.origin;
        let area = Rect::new(
            origin.x + local.origin.x,
            origin.y + local.origin.y,
            local.size.width,
            local.size.height,
        );
        let shown = element
            .xf
            .map_or(area, |xf| crate::scroll::map_rect(xf, area));
        match element.clip {
            Some(clip) => clip.intersection(shown),
            None => Some(shown),
        }
    }
}

impl Ui {
    /// The rectangles whose pixels changed since [`Self::clear_damage`], in
    /// logical window coordinates; `None` means the whole window. Hosts that
    /// keep earlier pixels redraw and present only these after a refresh.
    pub fn damage(&self) -> Option<Region<Rect>> {
        let state = self.read();
        Some(state.damage).filter(|d| !state.damage_full && !d.is_empty())
    }

    /// Marks the window's pixels as presented, after a successful present.
    pub fn clear_damage(&self) {
        let mut state = self.write();
        state.damage = Region::default();
        state.damage_full = false;
    }
}
