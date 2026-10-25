// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.

use crate::bar::FOOTPRINT;
use crate::{Result, control::MeasureCx, state::State};
use aegle_core::Dirty;
use aegle_layout::{Available, Length, Space};

impl State {
    /// Reserves each overflowing viewport's scrollbar footprint beyond its padding,
    /// so no control sits under a bar, and releases it when the overflow ends.
    /// Returns whether any viewport changed. Only the padding the user set on the
    /// side away from the bars is remembered (left and top, or right and top
    /// right to left): the bar side is derived from it.
    fn update_gutters(&mut self) -> Result<bool> {
        let mut changed = false;
        for index in 0..self.order.len() {
            let id = self.order[index];
            if !self.tree.get(id).unwrap().context.control.viewport() {
                continue;
            }
            let limit = self.scroll_limit(id);
            let rtl = self.rtl(id);
            let mut style = self.tree.get(id).unwrap().style().clone();
            let mut padding = style.padding();
            let (start, bar) = if rtl {
                (padding.right, &mut padding.left)
            } else {
                (padding.left, &mut padding.right)
            };
            let reserve = |base: Length, needed: bool| match (needed, base) {
                (true, Length::Px(value)) => Length::Px(value.max(FOOTPRINT)),
                (true, _) => Length::Px(FOOTPRINT),
                (false, base) => base,
            };
            let side = reserve(start, limit.y > 0.0);
            let bottom = reserve(padding.top, limit.x > 0.0);
            if *bar != side || padding.bottom != bottom {
                *bar = side;
                padding.bottom = bottom;
                style.set_padding(padding);
                aegle_layout::set_style(&mut self.tree, id, style)?;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Brings layout, geometry and paint records up to date; returns whether anything changed.
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
                if changes.paint {
                    self.tree.mark_dirty(id, Dirty::PAINT)?;
                }
                if changes.value || changes.layout || changes.selection || changes.policy {
                    self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
                    self.ime_dirty = true;
                }
            }
        }
        if self.tree.dirty(self.root)?.intersects(Dirty::LAYOUT) {
            // A viewport repaints its scroll bars only when its extent changes.
            let extents: Vec<_> = (0..self.order.len())
                .map(|index| self.order[index])
                .filter(|&id| self.tree.get(id).unwrap().context.control.viewport())
                .map(|id| (id, self.scroll_limit(id)))
                .collect();
            // A viewport that overflows reserves its scrollbar's footprint, which can
            // change wrapping, so lay out once more. Narrowing never removes an
            // overflow, so one extra pass is stable.
            for pass in 0..2 {
                let theme = &self.theme;
                let fonts = &self.fonts;
                aegle_layout::compute_with_baselines(
                    &mut self.tree,
                    self.root,
                    Space::definite(self.size),
                    |_, element, known, available| {
                        crate::idle::restore(element, fonts);
                        let padding = element.inset(theme);
                        let gap = element.theme_or(theme).gap;
                        let width = known.width.or(match available.width {
                            Available::Definite(w) => Some(w),
                            Available::MinContent => Some(0.0),
                            Available::MaxContent => None,
                        });
                        let size = element.control.measure(&MeasureCx {
                            fonts,
                            padding,
                            gap,
                            width,
                            rtl: element.rtl,
                        });
                        aegle_types::Size::new(
                            known.width.unwrap_or(size.width),
                            known.height.unwrap_or(size.height),
                        )
                    },
                    |_, element, size| {
                        crate::idle::restore(element, fonts);
                        element.control.baseline(size, element.inset(theme))
                    },
                )?;
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
                let size = node.bounds().size;
                let width = size.width;
                let element = &mut node.context;
                // Records are local, so only a new size or a viewport's scroll
                // extent changes pixels; a mere move is placed when drawing.
                let resized = (element.bounds.size.width, element.bounds.size.height)
                    != (size.width, size.height);
                let cx = MeasureCx {
                    fonts: &self.fonts,
                    padding: element.inset(&self.theme),
                    gap: element.theme_or(&self.theme).gap,
                    width: Some(width),
                    rtl: element.rtl,
                };
                // Released text is aligned when it is shown again.
                if !crate::idle::released(element) {
                    element.control.finalize(&cx);
                }
                let scrolled = extents
                    .iter()
                    .any(|&(viewport, limit)| viewport == id && self.scroll_limit(id) != limit);
                let dirty = if resized || scrolled {
                    Dirty::PAINT | Dirty::SEMANTICS
                } else {
                    Dirty::SEMANTICS
                };
                self.tree.mark_dirty(id, dirty)?;
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
            let viewport = element.control.text_viewport(element.bounds.size, padding);
            if let Some(field) = element.control.editor_mut() {
                let changes = field.editor_mut().take_changes();
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
        if let Some(target) = self.reveal_target.take()
            && self.tree.get(target).is_some()
            && self.usable(target)
        {
            self.reveal(target)?;
        }
        // An animation that starts outside a frame starts now, not at the
        // last frame, which may be long past.
        if self.animated.is_empty() {
            self.frame_time = std::time::Instant::now();
        }
        for index in 0..self.order.len() {
            let id = self.order[index];
            let element = &self.tree.get(id).unwrap().context;
            // Released text stays unrecorded until it is shown again.
            if self.tree.dirty(id)?.intersects(Dirty::PAINT) && !crate::idle::released(element) {
                self.record(id)?;
                self.tree.clear_dirty(id, Dirty::PAINT)?;
                self.repaint = true;
                self.damage_node(id, true);
            } else {
                self.damage_node(id, false);
            }
        }
        self.schedule_text()?;
        self.damage_groups();
        #[cfg(feature = "motion")]
        self.settle_scoped();
        Ok(std::mem::take(&mut self.repaint))
    }
}
