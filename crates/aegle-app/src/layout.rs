use crate::{
    Result,
    state::{Content, State},
};
use aegle_core::Dirty;
use aegle_layout::{AvailableSpace, Size};
use aegle_text::Alignment;

impl State {
    pub fn refresh(&mut self) -> Result<bool> {
        self.rebuild_order();
        for index in 0..self.order.len() {
            let id = self.order[index];
            if let Content::Field(field) = &mut self.tree.get_mut(id).unwrap().context.content {
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
            let padding = self.theme.padding;
            let gap = self.theme.gap;
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
                    let padding = element.inset(padding);
                    let width = known.width.or(match available.width {
                        AvailableSpace::Definite(w) => Some(w),
                        AvailableSpace::MinContent => Some(0.0),
                        AvailableSpace::MaxContent => None,
                    });
                    let measured = match &mut element.content {
                        Content::Label(text) => text
                            .reflow(
                                width.map(|w| (w - 2.0 * padding).max(0.0)),
                                Alignment::Start,
                            )
                            .map(|s| {
                                aegle_types::Size::new(
                                    s.width + 2.0 * padding,
                                    s.height + 2.0 * padding,
                                )
                            }),
                        Content::Button(_, text) => Ok(aegle_types::Size::new(
                            text.size().width + 2.0 * padding,
                            text.size().height + 2.0 * padding,
                        )),
                        Content::Toggle(toggle) => Ok(aegle_types::Size::new(
                            if toggle.switch { 36.0 } else { 18.0 }
                                + if toggle.text.text().is_empty() {
                                    0.0
                                } else {
                                    gap + toggle.text.size().width
                                }
                                + 2.0 * padding,
                            toggle.text.size().height.max(20.0) + 2.0 * padding,
                        )),
                        Content::Slider(_) | Content::Progress(_) => {
                            Ok(aegle_types::Size::new(160.0, 20.0 + 2.0 * padding))
                        }
                        Content::Field(field) => fonts
                            .borrow_mut()
                            .edit(field.editor_mut())
                            .reflow(
                                width.map(|w| (w - 2.0 * padding).max(0.0)),
                                Alignment::Start,
                            )
                            .map(|s| {
                                aegle_types::Size::new(
                                    s.width + 2.0 * padding,
                                    s.height + 2.0 * padding,
                                )
                            }),
                        Content::Container | Content::Scroll(_) => Ok(aegle_types::Size::default()),
                    };
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
            for index in 0..self.order.len() {
                let id = self.order[index];
                let node = self.tree.get_mut(id).unwrap();
                // Ensure retained text geometry uses final layout constraints, even
                // when Taffy's measurement callback last evaluated an intrinsic pass.
                let width = node.bounds().size.width;
                let padding = node.context.inset(padding);
                match &mut node.context.content {
                    Content::Label(text) => {
                        text.reflow(Some((width - 2.0 * padding).max(0.0)), Alignment::Start)?;
                    }
                    Content::Field(field) => {
                        self.fonts
                            .borrow_mut()
                            .edit(field.editor_mut())
                            .reflow(Some((width - 2.0 * padding).max(0.0)), Alignment::Start)?;
                    }
                    _ => {}
                }
                self.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            self.geometry_dirty = true;
            self.ime_dirty = true;
            self.repaint = true;
        }
        self.update_geometry()?;
        for index in 0..self.order.len() {
            let id = self.order[index];
            let focused = self.focus.current(&self.tree) == Some(id);
            let element = &mut self.tree.get_mut(id).unwrap().context;
            if let Content::Field(field) = &mut element.content {
                let changes = field.editor_mut().take_changes();
                let viewport = aegle_types::Size::new(
                    (element.bounds.size.width
                        - element.padding.unwrap_or(self.theme.padding) * 2.0)
                        .max(0.0),
                    (element.bounds.size.height
                        - element.padding.unwrap_or(self.theme.padding) * 2.0)
                        .max(0.0),
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
