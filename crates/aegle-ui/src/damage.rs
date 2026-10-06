//! The window area whose pixels changed, so a host can redraw and present
//! only that part. Re-recorded nodes add their presented area; any change
//! that moves geometry, structure or clips damages the whole window.

use crate::{Result, Ui, UiError, state::State};
use aegle_core::NodeId;
use aegle_types::Rect;

impl State {
    /// Adds the presented area of a node whose record changed, including its
    /// shadow. Geometry is unchanged, so the old area is the same.
    pub(crate) fn damage_node(&mut self, id: NodeId) {
        let element = &self.tree.get(id).unwrap().context;
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
        let shown = match element.clip {
            Some(clip) => match clip.intersection(shown) {
                Some(shown) => shown,
                None => return,
            },
            None => shown,
        };
        self.damage = Some(self.damage.map_or(shown, |d| d.union(shown)));
    }
}

impl Ui {
    /// The area whose pixels changed since [`Self::clear_damage`], in logical
    /// window coordinates; `None` means the whole window. Hosts that keep
    /// earlier pixels redraw and present only this area after a refresh.
    pub fn damage(&self) -> Result<Option<Rect>> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| UiError::ReentrantAccess)?;
        Ok(state.damage.filter(|_| !state.damage_full))
    }

    /// Marks the window's pixels as presented, after a successful present.
    pub fn clear_damage(&self) -> Result {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.damage = None;
        state.damage_full = false;
        Ok(())
    }
}
