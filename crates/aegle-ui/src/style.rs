// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.
#![allow(missing_docs)]

use crate::state::State;
use crate::{Appearance, Result, Skin, Style, UiError, VisualState};
use aegle_core::{Dirty, NodeId};
use aegle_text::TextStyle;

/// Stored only for nodes with an explicit visual or typography override.
#[derive(Clone, Default)]
pub struct Decoration {
    pub skin: Option<Skin>,
    pub style: Style,
    pub font_size: Option<f32>,
    pub cursor: Option<aegle_types::Cursor>,
    pub shadow: Option<crate::Shadow>,
    pub gradient: Option<aegle_scene::Gradient>,
}

impl State {
    pub fn visual_state(&self, id: NodeId) -> VisualState {
        let control = &self.tree.get(id).unwrap().context.control;
        let visual = control.visual();
        VisualState {
            kind: control.kind(),
            enabled: self.usable(id),
            hovered: visual.hovered.unwrap_or(self.hover == Some(id)),
            pressed: visual.pressed,
            focused: self.focus.current(&self.tree) == Some(id),
            read_only: visual.read_only,
            checked: visual.checked,
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
            d.skin.is_none()
                && d.style == Style::default()
                && d.font_size.is_none()
                && d.cursor.is_none()
                && d.shadow.is_none()
                && d.gradient.is_none()
        }) {
            self.decorations.remove(&id);
        }
    }

    pub fn set_style(&mut self, id: NodeId, style: Style) -> Result {
        style.validate()?;
        let scope = self.tree.get(id).unwrap().context.control.style_scope();
        let (button, field) = (scope.button_like, scope.editor);
        if ((style.selection.is_some() || style.caret.is_some()) && !field)
            || (style.indicator.is_some() && !scope.indicator)
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
        let control = &self.tree.get(id).unwrap().context.control;
        if control.paragraph().is_none() && control.editor().is_none() {
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
        let control = &mut self.tree.get_mut(id).unwrap().context.control;
        if let Some(text) = control.paragraph_mut() {
            self.fonts.borrow_mut().restyle(text, &style)?;
        } else if let Some(field) = control.editor_mut() {
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
