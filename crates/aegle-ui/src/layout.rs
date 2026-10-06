// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.
#![allow(missing_docs)]

use crate::bar::FOOTPRINT;
use crate::{Result, control::MeasureCx, state::State};
use aegle_core::Dirty;
use aegle_layout::{AvailableSpace, LengthPercentage, Size};

impl State {
    /// Reserves each overflowing viewport's scrollbar footprint beyond its padding,
    /// so no control sits under a bar, and releases it when the overflow ends.
    /// Returns whether any viewport changed. Only the leading padding the user set
    /// is remembered: the trailing edge is derived from it.
    fn update_gutters(&mut self) -> Result<bool> {
        let mut changed = false;
        for index in 0..self.order.len() {
            let id = self.order[index];
            if !self.tree.get(id).unwrap().context.control.viewport() {
                continue;
            }
            let limit = self.scroll_limit(id);
            let mut style = self.tree.get(id).unwrap().style().clone();
            let (left, top) = (style.padding.left, style.padding.top);
            let reserve =
                |base: LengthPercentage, needed: bool| match (needed, base.into_raw().value()) {
                    (true, value) => LengthPercentage::length(value.max(FOOTPRINT)),
                    _ => base,
                };
            let right = reserve(left, limit.y > 0.0);
            let bottom = reserve(top, limit.x > 0.0);
            if style.padding.right != right || style.padding.bottom != bottom {
                style.padding.right = right;
                style.padding.bottom = bottom;
                aegle_layout::set_style(&mut self.tree, id, style)?;
                changed = true;
            }
        }
        Ok(changed)
    }

    pub fn refresh(&mut self) -> Result<bool> {
        #[cfg(feature = "motion")]
        self.start_offsets();
        self.rebuild_order();
        for index in 0..self.order.len() {
            let id = self.order[index];
            if let Some(field) = self.tree.get_mut(id).unwrap().context.control.editor_mut() {
                let changes = field.editor_mut().take_changes();
                self.tree.get_mut(id).unwrap().context.ensure_caret |=
                    changes.layout || changes.selection;
                if changes.layout {
                    self.tree.mark_dirty(id, Dirty::LAYOUT)?;
                }
                if changes.value || changes.layout || changes.selection || changes.policy {
                    self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
                    self.ime_dirty = true;
                }
            }
        }
        if self.tree.dirty(self.root)?.intersects(Dirty::LAYOUT) {
            // A viewport that overflows reserves its scrollbar's footprint, which can
            // change wrapping, so lay out once more. Narrowing never removes an
            // overflow, so one extra pass is stable.
            for pass in 0..2 {
                let theme = &self.theme;
                let fonts = &self.fonts;
                let mut error = None;
                aegle_layout::compute(
                    &mut self.tree,
                    self.root,
                    Size {
                        width: AvailableSpace::Definite(self.size.width),
                        height: AvailableSpace::Definite(self.size.height),
                    },
                    |_, element, known, available| {
                        let padding = element.inset(theme);
                        let gap = element.theme_or(theme).gap;
                        let width = known.width.or(match available.width {
                            AvailableSpace::Definite(w) => Some(w),
                            AvailableSpace::MinContent => Some(0.0),
                            AvailableSpace::MaxContent => None,
                        });
                        let measured = element.control.measure(&MeasureCx {
                            fonts,
                            padding,
                            gap,
                            width,
                        });
                        match measured {
                            Ok(size) => Size {
                                width: known.width.unwrap_or(size.width),
                                height: known.height.unwrap_or(size.height),
                            },
                            Err(cause) => {
                                error = Some(cause);
                                Size::ZERO
                            }
                        }
                    },
                )?;
                if let Some(error) = error {
                    return Err(error.into());
                }
                if pass == 0 && self.update_gutters()? {
                    continue;
                }
                break;
            }
            for index in 0..self.order.len() {
                let id = self.order[index];
                let node = self.tree.get_mut(id).unwrap();
                // Ensure retained text geometry uses final layout constraints, even
                // when Taffy's measurement callback last evaluated an intrinsic pass.
                let width = node.bounds().size.width;
                let padding = node.context.inset(&self.theme);
                node.context.control.finalize(&self.fonts, width, padding)?;
                self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            self.geometry_dirty = true;
            self.ime_dirty = true;
            self.repaint = true;
        }
        self.update_geometry()?;
        let mut placed = false;
        for hook in self.hooks.clone() {
            if let Some(place) = hook.place {
                placed |= place(self);
            }
        }
        if placed {
            self.geometry_dirty = true;
            self.repaint = true;
            self.update_geometry()?;
        }
        for index in 0..self.order.len() {
            let id = self.order[index];
            let focused = self.focus.current(&self.tree) == Some(id);
            let element = &mut self.tree.get_mut(id).unwrap().context;
            let padding = element.inset(&self.theme);
            if let Some(field) = element.control.editor_mut() {
                let changes = field.editor_mut().take_changes();
                let viewport = aegle_types::Size::new(
                    (element.bounds.size.width - padding * 2.0).max(0.0),
                    (element.bounds.size.height - padding * 2.0).max(0.0),
                );
                let caret = field.editor().ime_rect();
                let size = field.editor().size();
                if std::mem::take(&mut element.ensure_caret) || changes.layout || changes.selection
                {
                    if focused {
                        self.reveal_target = Some(id);
                    }
                    element.scroll.x = element
                        .scroll
                        .x
                        .min(caret.origin.x)
                        .max(caret.origin.x + caret.size.width - viewport.width);
                    element.scroll.y = element
                        .scroll
                        .y
                        .min(caret.origin.y)
                        .max(caret.origin.y + caret.size.height - viewport.height);
                }
                element.scroll.x = element.scroll.x.clamp(
                    0.0,
                    (size.width + caret.size.width - viewport.width).max(0.0),
                );
                element.scroll.y = element
                    .scroll
                    .y
                    .clamp(0.0, (size.height - viewport.height).max(0.0));
            }
        }
        if let Some(target) = self.reveal_target.take() {
            if self.tree.get(target).is_some() && self.usable(target) {
                self.reveal(target)?;
            }
        }
        for index in 0..self.order.len() {
            let id = self.order[index];
            if self.tree.dirty(id)?.intersects(Dirty::PAINT) {
                self.record(id)?;
                self.tree.clear_dirty(id, Dirty::PAINT)?;
                self.repaint = true;
            }
        }
        Ok(std::mem::take(&mut self.repaint))
    }
}
