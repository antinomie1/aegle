//! Text a UI does not show gives up its layout while the application's idle
//! text exceeds the budget of the shared [`TextSystem`]: the least recently
//! shown first, shaped again when it is shown or laid out.

use std::{cell::RefCell, collections::HashMap, time::Instant};

use aegle_core::{Dirty, NodeId};
use aegle_scene::Scene;
use aegle_text::TextSystem;

use crate::{Result, control::MeasureCx, state::Element, state::State};

/// One UI's share of the idle text budget.
#[derive(Default)]
pub struct Idle {
    /// Idle bytes last reported to the text system.
    reported: usize,
    /// Paragraphs released at the last scan.
    released: usize,
    /// When each text node was last shown.
    shown: HashMap<NodeId, Instant>,
    /// Idle paragraphs found by a scan, reused.
    scratch: Vec<(Option<Instant>, NodeId, usize)>,
}

impl Idle {
    pub(crate) fn forget(&mut self, id: NodeId) {
        self.shown.remove(&id);
    }
}

/// Shapes the element's paragraph again if it was released, before it is
/// measured, aligned or drawn.
pub(crate) fn restore(element: &mut Element, fonts: &RefCell<TextSystem>) {
    if let Some(paragraph) = element.control.paragraph_mut()
        && paragraph.is_released()
    {
        fonts.borrow_mut().restore(paragraph);
    }
}

/// Whether the element's paragraph is released.
pub(crate) fn released(element: &Element) -> bool {
    element.control.paragraph().is_some_and(|p| p.is_released())
}

impl State {
    /// Whether a node's records reach the window, as [`crate::Ui::visit_scenes`]
    /// draws them.
    pub(crate) fn shown(&self, id: NodeId) -> bool {
        let element = &self.tree.get(id).unwrap().context;
        let shown = element.xf.map_or(element.bounds, |xf| {
            crate::scroll::map_rect(xf, element.bounds)
        });
        element.effective_visible
            && element
                .clip
                .is_none_or(|clip| clip.intersection(shown).is_some())
    }

    /// After records are refreshed: shows released text that is shown again
    /// and, while the application's idle text is over budget, releases this
    /// UI's least recently shown paragraphs with their records.
    pub(crate) fn schedule_text(&mut self) -> Result {
        let fonts = std::rc::Rc::clone(&self.fonts);
        let budget = fonts.borrow().idle_budget();
        if budget.is_none() && self.idle.reported == 0 && self.idle.released == 0 {
            return Ok(());
        }
        let now = self.frame_time;
        let mut idle = std::mem::take(&mut self.idle.scratch);
        let (mut bytes, mut released) = (0, 0);
        for index in 0..self.order.len() {
            let id = self.order[index];
            let Some(paragraph) = self.tree.get(id).unwrap().context.control.paragraph() else {
                continue;
            };
            let (text, is_released) = (paragraph.text().len(), paragraph.is_released());
            if self.shown(id) {
                self.idle.shown.insert(id, now);
                if is_released {
                    self.show_released(id, &fonts)?;
                }
            } else if is_released {
                released += 1;
            } else if budget.is_some() {
                bytes += text;
                idle.push((self.idle.shown.get(&id).copied(), id, text));
            }
        }
        let mut system = fonts.borrow_mut();
        system.report_idle(self.idle.reported, bytes);
        self.idle.reported = bytes;
        if system.over_idle_budget() {
            // Never shown sorts first.
            idle.sort_unstable_by_key(|&(shown, ..)| shown);
            for &(_, id, text) in &idle {
                if !system.over_idle_budget() {
                    break;
                }
                let element = &mut self.tree.get_mut(id).unwrap().context;
                element.control.paragraph_mut().unwrap().release();
                element.scene = Scene::default();
                system.report_idle(text, 0);
                self.idle.reported -= text;
                released += 1;
            }
        }
        idle.clear();
        self.idle.scratch = idle;
        self.idle.released = released;
        Ok(())
    }

    /// Shapes, aligns and records released text that is shown again.
    fn show_released(&mut self, id: NodeId, fonts: &RefCell<TextSystem>) -> Result {
        let element = &mut self.tree.get_mut(id).unwrap().context;
        restore(element, fonts);
        element.control.finalize(&MeasureCx {
            fonts,
            padding: element.inset(&self.theme),
            gap: element.theme_or(&self.theme).gap,
            width: Some(element.bounds.size.width),
            rtl: element.rtl,
        });
        self.record(id)?;
        self.tree.clear_dirty(id, Dirty::PAINT)?;
        self.damage_node(id, true);
        self.repaint = true;
        Ok(())
    }

    /// Withdraws this UI's report, as when its tree goes away.
    pub(crate) fn withdraw_text(&mut self) {
        let reported = std::mem::take(&mut self.idle.reported);
        self.fonts.borrow_mut().report_idle(reported, 0);
        self.idle.shown.clear();
        self.idle.released = 0;
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.withdraw_text();
    }
}
