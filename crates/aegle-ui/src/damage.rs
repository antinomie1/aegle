//! The window area whose pixels changed, so a host can redraw and present
//! only that part. Re-recorded, moved, resized or newly clipped nodes add
//! their old and new presented areas; structure changes and resizing the
//! window damage all of it.

use crate::{Result, Ui, UiError, state::State};
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

    /// The presented window area of a node's records, including its shadow;
    /// `None` when it draws nothing visible.
    fn painted_area(&self, id: NodeId) -> Option<Rect> {
        let element = &self.tree.get(id).unwrap().context;
        let empty = element.scene.commands().is_empty()
            && element
                .overlay
                .as_ref()
                .is_none_or(|o| o.commands().is_empty());
        if !element.effective_visible || empty {
            return None;
        }
        let mut area = element.bounds;
        if let Some(shadow) = self.decorations.get(&id).and_then(|d| d.shadow) {
            // A Gaussian shadow is invisible beyond three standard deviations.
            let reach = shadow.spread.max(0.0) + shadow.blur * 3.0;
            let cast = Rect::new(
                area.origin.x + shadow.offset.x - reach,
                area.origin.y + shadow.offset.y - reach,
                area.size.width + reach * 2.0,
                area.size.height + reach * 2.0,
            );
            area = area.union(cast);
        }
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
    pub fn damage(&self) -> Result<Option<Region<Rect>>> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| UiError::ReentrantAccess)?;
        Ok(Some(state.damage).filter(|d| !state.damage_full && !d.is_empty()))
    }

    /// Marks the window's pixels as presented, after a successful present.
    pub fn clear_damage(&self) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.damage = Region::default();
        state.damage_full = false;
        Ok(())
    }
}
