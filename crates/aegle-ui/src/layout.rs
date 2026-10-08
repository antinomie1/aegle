// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.

use crate::bar::FOOTPRINT;
use crate::{Result, control::MeasureCx, state::State};
use aegle_core::Dirty;
use aegle_layout::{AvailableSpace, LengthPercentage, Size};

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
            let padding = &mut style.padding;
            let (start, bar) = if rtl {
                (padding.right, &mut padding.left)
            } else {
                (padding.left, &mut padding.right)
            };
            let reserve =
                |base: LengthPercentage, needed: bool| match (needed, base.into_raw().value()) {
                    (true, value) => LengthPercentage::length(value.max(FOOTPRINT)),
                    _ => base,
                };
            let side = reserve(start, limit.y > 0.0);
            let bottom = reserve(padding.top, limit.x > 0.0);
            if *bar != side || padding.bottom != bottom {
                *bar = side;
                padding.bottom = bottom;
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
                let mut error = None;
                aegle_layout::compute_with_baselines(
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
                            rtl: element.rtl,
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
                    |_, element, size| {
                        let size = aegle_types::Size::new(size.width, size.height);
                        element.control.baseline(size, element.inset(theme))
                    },
                )?;
                if let Some(error) = error {
                    return Err(error);
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
                element.control.finalize(&cx)?;
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
            if self.tree.dirty(id)?.intersects(Dirty::PAINT) {
                self.record(id)?;
                self.tree.clear_dirty(id, Dirty::PAINT)?;
                self.repaint = true;
                self.damage_node(id, true);
            } else {
                self.damage_node(id, false);
            }
        }
        #[cfg(feature = "motion")]
        self.settle_scoped();
        Ok(std::mem::take(&mut self.repaint))
    }
}
