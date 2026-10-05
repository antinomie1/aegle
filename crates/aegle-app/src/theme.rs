use crate::{Result, Theme, Ui, UiError, state::Content};
use aegle_core::Dirty;
use aegle_layout::{Dimension, Edges, LengthPercentage, LengthPercentageAuto};
use aegle_text::TextStyle;

impl Ui {
    /// Replaces the theme while preserving editor text, focus, selection and preedit.
    /// Palette-only changes do not reshape text or recompute layout. Local layout
    /// setters remain authoritative over changed theme defaults.
    pub fn set_theme(&self, theme: Theme) -> Result {
        theme.validate()?;
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        let old = state.theme;
        if old == theme {
            return Ok(());
        }
        state.rebuild_order();
        let fonts = std::rc::Rc::clone(&state.fonts);
        for index in 0..state.order.len() {
            let id = state.order[index];
            let is_root = id == state.root;
            let font_changed = theme.font_size != old.font_size
                && state
                    .decorations
                    .get(&id)
                    .and_then(|d| d.font_size)
                    .is_none();
            let node = state.tree.get_mut(id).unwrap();
            let mut style = node.style().clone();
            if font_changed {
                let text_style = TextStyle {
                    size: theme.font_size,
                    color: theme.foreground,
                    ..Default::default()
                };
                if let Some(text) = node.context.content.paragraph_mut() {
                    fonts.borrow_mut().restyle(text, &text_style)?;
                } else if let Content::Field(field) = &mut node.context.content {
                    fonts
                        .borrow_mut()
                        .edit(field.editor_mut())
                        .restyle(&text_style)?;
                }
            }
            match &node.context.content {
                Content::Container | Content::Scroll => {
                    if node.context.local_layout & 4 == 0 {
                        let gap = LengthPercentage::length(theme.gap);
                        style.gap = aegle_layout::Size {
                            width: gap,
                            height: gap,
                        };
                    }
                    if is_root && node.context.local_layout & 2 == 0 {
                        let p = LengthPercentage::length(theme.padding);
                        style.padding = Edges {
                            left: p,
                            right: p,
                            top: p,
                            bottom: p,
                        };
                    }
                }
                Content::Button(..) | Content::Toggle(_) | Content::Slider(_)
                    if node.context.local_layout & 1 == 0 =>
                {
                    style.size.height = Dimension::length(theme.control_height)
                }
                Content::Progress(_) if node.context.local_layout & 1 == 0 => {
                    style.size.height = Dimension::length(theme.control_height / 2.0)
                }
                Content::Field(field) => {
                    if node.context.local_layout & 1 == 0 {
                        style.size.height = Dimension::length(
                            theme.control_height
                                * if field.editor().is_multiline() {
                                    4.0
                                } else {
                                    1.0
                                },
                        );
                    }
                    if node.context.local_layout & 8 == 0 {
                        style.min_size.height = LengthPercentageAuto::length(theme.control_height);
                    }
                }
                _ => {}
            }
            if style != *node.style() {
                aegle_layout::set_style(&mut state.tree, id, style)?;
            }
            let custom_skin = state.decorations.get(&id).is_some_and(|d| d.skin.is_some());
            let toggle_gap_changed = theme.gap != old.gap
                && matches!(
                    state.tree.get(id).unwrap().context.content,
                    Content::Toggle(_)
                );
            let dirty = if font_changed || theme.padding != old.padding || toggle_gap_changed {
                Dirty::ALL
            } else if theme.foreground != old.foreground || theme.muted != old.muted || custom_skin
            {
                Dirty::PAINT | Dirty::SEMANTICS
            } else {
                Dirty::PAINT
            };
            state.tree.mark_dirty(id, dirty)?;
        }
        state.theme = theme;
        state.repaint = true;
        state.ime_dirty = true;
        Ok(())
    }
}
