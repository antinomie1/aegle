use crate::state::{Content, State};
use crate::{Appearance, ControlKind, Result, Skin, Style, UiError, VisualState};
use aegle_core::{Dirty, NodeId};
use aegle_text::TextStyle;

/// Stored only for nodes with an explicit visual or typography override.
#[derive(Clone, Copy, Default)]
pub(crate) struct Decoration {
    pub skin: Option<Skin>,
    pub style: Style,
    pub font_size: Option<f32>,
}

impl State {
    pub fn visual_state(&self, id: NodeId) -> VisualState {
        let content = &self.tree.get(id).unwrap().context.content;
        let (kind, pressed, read_only) = match content {
            Content::Container | Content::Scroll(_) | Content::Image(_) | Content::Canvas(_) => {
                (ControlKind::Container, false, false)
            }
            Content::Label(_) => (ControlKind::Label, false, false),
            Content::Button(button, _) => (ControlKind::Button, button.is_pressed(), false),
            Content::Field(field) => (ControlKind::TextField, false, field.editor().is_read_only()),
            Content::Toggle(toggle) => (
                if toggle.switch {
                    ControlKind::Switch
                } else {
                    ControlKind::CheckBox
                },
                toggle.control.is_pressed(),
                false,
            ),
            Content::Slider(slider) => (ControlKind::Slider, slider.is_pressed(), false),
            Content::Progress(_) => (ControlKind::Progress, false, false),
        };
        VisualState {
            kind,
            enabled: self.usable(id),
            hovered: match content {
                Content::Button(button, _) => button.is_hovered(),
                Content::Toggle(toggle) => toggle.control.is_hovered(),
                Content::Slider(slider) => slider.is_hovered(),
                _ => self.hover == Some(id),
            },
            pressed,
            focused: self.focus.current(&self.tree) == Some(id),
            read_only,
            checked: matches!(content, Content::Toggle(toggle) if toggle.control.is_checked()),
        }
    }

    pub fn appearance(&self, id: NodeId) -> Result<Appearance> {
        self.appearance_for(id, self.visual_state(id))
    }

    pub fn appearance_for(&self, id: NodeId, state: VisualState) -> Result<Appearance> {
        let decoration = self.decorations.get(&id);
        let skin = decoration.and_then(|d| d.skin).unwrap_or(Appearance::new);
        let mut appearance = skin(self.theme_of(id), state);
        if let Some(decoration) = decoration {
            decoration.style.apply(&mut appearance, state);
        }
        appearance.validate()?;
        if !state.enabled || !state.focused {
            appearance.focus_width = 0.0;
        }
        Ok(appearance)
    }

    pub fn text_style(&self, id: NodeId) -> TextStyle<'static> {
        let theme = self.theme_of(id);
        TextStyle {
            size: self
                .decorations
                .get(&id)
                .and_then(|d| d.font_size)
                .unwrap_or(theme.font_size),
            ..crate::state::text_style(theme)
        }
    }

    pub fn trim_decoration(&mut self, id: NodeId) {
        if self.decorations.get(&id).is_some_and(|d| {
            d.skin.is_none() && d.style == Style::default() && d.font_size.is_none()
        }) {
            self.decorations.remove(&id);
        }
    }

    pub fn set_style(&mut self, id: NodeId, style: Style) -> Result {
        style.validate()?;
        let content = &self.tree.get(id).unwrap().context.content;
        let button = matches!(
            content,
            Content::Button(..) | Content::Toggle(_) | Content::Slider(_)
        );
        let field = matches!(content, Content::Field(_));
        if ((style.selection.is_some() || style.caret.is_some()) && !field)
            || (style.indicator.is_some()
                && !matches!(
                    content,
                    Content::Toggle(_) | Content::Slider(_) | Content::Progress(_)
                ))
            || (style.pressed_background.is_some() && !button)
            || ((style.hover_background.is_some()
                || style.focus_color.is_some()
                || style.focus_width.is_some())
                && !(button || field))
        {
            return Err(UiError::WrongKind.into());
        }
        let old = self
            .decorations
            .get(&id)
            .map_or(Style::default(), |d| d.style);
        if old == style {
            return Ok(());
        }
        self.decorations.entry(id).or_default().style = style;
        self.trim_decoration(id);
        let dirty = if old.foreground != style.foreground
            || old.disabled_foreground != style.disabled_foreground
        {
            Dirty::PAINT | Dirty::SEMANTICS
        } else {
            Dirty::PAINT
        };
        self.tree.mark_dirty(id, dirty)?;
        Ok(())
    }

    pub fn set_font_size(&mut self, id: NodeId, size: Option<f32>) -> Result {
        if size.is_some_and(|v| !v.is_finite() || v <= 0.0) {
            return Err(UiError::InvalidValue.into());
        }
        let content = &self.tree.get(id).unwrap().context.content;
        if content.paragraph().is_none() && !matches!(content, Content::Field(_)) {
            return Err(UiError::WrongKind.into());
        }
        let old = self.decorations.get(&id).and_then(|d| d.font_size);
        if old == size {
            return Ok(());
        }
        let theme = self.theme_of(id);
        let style = TextStyle {
            size: size.unwrap_or(theme.font_size),
            ..crate::state::text_style(theme)
        };
        let content = &mut self.tree.get_mut(id).unwrap().context.content;
        if let Some(text) = content.paragraph_mut() {
            self.fonts.borrow_mut().restyle(text, &style)?;
        } else if let Content::Field(field) = content {
            self.fonts
                .borrow_mut()
                .edit(field.editor_mut())
                .restyle(&style)?;
        }
        self.decorations.entry(id).or_default().font_size = size;
        self.trim_decoration(id);
        self.tree.mark_dirty(id, Dirty::ALL)?;
        self.ime_dirty = true;
        Ok(())
    }

    pub fn dirty_visual_state(&mut self, id: NodeId) -> Result {
        let dirty = if self.decorations.get(&id).is_some_and(|d| d.skin.is_some()) {
            // A custom skin can change foreground as a function of any state.
            Dirty::PAINT | Dirty::SEMANTICS
        } else {
            Dirty::PAINT
        };
        self.tree.mark_dirty(id, dirty)?;
        Ok(())
    }
}
