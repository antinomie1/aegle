// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.
#![allow(missing_docs)]

//! Hover tracking under a moving or stationary pointer.

use aegle_controls::{Modifiers, PointerId, PointerKind};
use aegle_core::NodeId;
use aegle_types::Point;

use crate::{Result, control::InputCx, state::State};

impl State {
    /// Geometry may move under a stationary pointer. Refresh hover without
    /// generating slider drag updates, text selections or application callbacks.
    pub fn rehit_pointer(&mut self) -> Result {
        if self.drag.is_some() {
            return Ok(());
        }
        if let Some((id, position)) = self.pointer {
            let hit = self.hit(position);
            self.update_hover(hit, id, position)?;
            self.sync_hover(id, position)?;
        }
        Ok(())
    }

    pub(crate) fn update_hover(
        &mut self,
        hit: Option<NodeId>,
        id: PointerId,
        position: Point,
    ) -> Result {
        if self.hover == hit {
            return Ok(());
        }
        if let Some(old) = self.hover.take() {
            let input =
                self.pointer_input(old, id, PointerKind::Leave, position, Modifiers::default());
            let outcome = self.control(old, input)?;
            self.effects(old, outcome)?;
            self.dirty_visual_state(old)?;
        }
        self.hover = hit;
        self.hovered(hit)?;
        if let Some(hit) = hit {
            self.dirty_visual_state(hit)?;
        }
        Ok(())
    }

    pub(crate) fn sync_hover(&mut self, id: PointerId, position: Point) -> Result {
        if let Some(hit) = self.hover {
            let owns_pointer = self.capture.is_none_or(|capture| capture == (id, hit));
            if owns_pointer {
                let input =
                    self.pointer_input(hit, id, PointerKind::Move, position, Modifiers::default());
                let fonts = self.fonts.clone();
                let theme = self.theme;
                let element = &mut self.tree.get_mut(hit).unwrap().context;
                let (size, padding) = (element.bounds.size, element.inset(&theme));
                let outcome = element.control.hover(
                    &mut InputCx {
                        fonts: &mut fonts.borrow_mut(),
                        size,
                        padding,
                        time: self.input_time,
                        deferred: &mut Vec::new(),
                    },
                    id,
                    input,
                )?;
                self.effects(hit, outcome)?;
            }
        }
        Ok(())
    }
    /// Tells the installed control libraries the hovered control changed.
    pub(crate) fn hovered(&mut self, hit: Option<NodeId>) -> Result {
        for hook in self.hooks.clone() {
            if let Some(hover) = hook.hover {
                hover(self, hit)?;
            }
        }
        Ok(())
    }
}
