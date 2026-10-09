// The engine state's fields and methods are the authoring surface for control
// libraries; the contract is described in `control` and on `State`.

use crate::state::State;
use crate::{Appearance, Result, Style, Theme, UiError, VisualState};
use aegle_core::{Dirty, NodeId};
use aegle_text::{FontStyle, FontWeight, TextStyle};
use aegle_theme::Accepts;
use aegle_theme::Font;

/// Stored only for nodes with an explicit visual or typography override.
#[derive(Clone, Default)]
pub struct Decoration {
    pub style: Style,
    pub font_size: Option<f32>,
    pub font: Option<Font>,
    pub cursor: Option<aegle_types::Cursor>,
    pub shadow: Option<crate::Shadow>,
    pub gradient: Option<aegle_scene::Gradient>,
}

impl Decoration {
    /// Whether it holds no override.
    pub fn is_empty(&self) -> bool {
        self.style == Style::default()
            && self.font_size.is_none()
            && self.font.is_none()
            && self.cursor.is_none()
            && self.shadow.is_none()
            && self.gradient.is_none()
    }
}

impl State {
    /// The control's current interaction state for skin resolution.
    pub fn visual_state(&self, id: NodeId) -> VisualState {
        let control = &self.tree.get(id).unwrap().context.control;
        let visual = control.visual();
        VisualState {
            kind: control.kind(),
            enabled: self.usable(id),
            hovered: visual.hovered.unwrap_or(self.hover == Some(id)),
            pressed: visual.pressed,
            focused: self.focus.current(&self.tree) == Some(id)
                && (self.focus_visible || control.editor().is_some()),
            read_only: visual.read_only,
            checked: visual.checked,
        }
    }

    /// The resolved appearance of `id` in its current state.
    pub fn appearance(&self, id: NodeId) -> Result<Appearance> {
        self.appearance_for(id, self.visual_state(id))
    }

    /// The resolved appearance of `id` in `state`, after skin and local overrides.
    pub fn appearance_for(&self, id: NodeId, state: VisualState) -> Result<Appearance> {
        let decoration = self.decorations.get(&id);
        let element = &self.tree.get(id).unwrap().context;
        let skin = element.skin.unwrap_or(state.kind.skin);
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

    /// The text style of `id` under its resolved theme.
    pub fn text_style(&self, id: NodeId) -> TextStyle<'static> {
        self.text_style_in(id, self.theme_of(id))
    }

    /// The text style of `id` under `theme`, with its local size and font.
    pub fn text_style_in(&self, id: NodeId, theme: &Theme) -> TextStyle<'static> {
        let mut style = face(theme, self.decorations.get(&id));
        let control = &self.tree.get(id).unwrap().context.control;
        control.text_role(&mut style);
        style
    }

    /// Drops the decoration of `id` once it holds no override.
    pub fn trim_decoration(&mut self, id: NodeId) {
        if self.decorations.get(&id).is_some_and(Decoration::is_empty) {
            self.decorations.remove(&id);
        }
    }

    /// Validates and sets the local style overrides of `id`.
    pub fn set_style(&mut self, id: NodeId, style: Style) -> Result {
        style.validate()?;
        let accepts = self.tree.get(id).unwrap().context.control.kind().accepts;
        if ((style.selection.is_some() || style.caret.is_some())
            && !accepts.contains(Accepts::EDITOR))
            || (style.indicator.is_some() && !accepts.contains(Accepts::INDICATOR))
            || (style.pressed_background.is_some() && !accepts.contains(Accepts::PRESSED))
            || ((style.hover_background.is_some()
                || style.focus_color.is_some()
                || style.focus_width.is_some())
                && !accepts.contains(Accepts::INTERACTIVE))
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

    /// Replaces local paint overrides through `edit`.
    pub fn edit_style(&mut self, id: NodeId, edit: impl FnOnce(&mut Style)) -> Result {
        let mut style = self
            .decorations
            .get(&id)
            .map_or(Style::default(), |d| d.style);
        edit(&mut style);
        self.set_style(id, style)
    }

    /// Sets or clears the local font size; it must be finite and positive.
    pub fn set_font_size(&mut self, id: NodeId, size: Option<f32>) -> Result {
        if size.is_some_and(|v| !v.is_finite() || v <= 0.0) {
            return Err(UiError::InvalidValue.into());
        }
        self.set_typeface(id, |d| d.font_size = size)
    }

    /// Sets or clears the local font.
    pub fn set_font(&mut self, id: NodeId, font: Option<Font>) -> Result {
        if font.is_some_and(|f| !f.is_valid()) {
            return Err(UiError::InvalidValue.into());
        }
        self.set_typeface(id, |d| d.font = font)
    }

    /// Changes the local size or font of a text-bearing control, reshaping
    /// its text and keeping selection and preedit.
    fn set_typeface(&mut self, id: NodeId, change: impl FnOnce(&mut Decoration)) -> Result {
        let control = &self.tree.get(id).unwrap().context.control;
        if control.paragraph().is_none() && control.editor().is_none() {
            return Err(UiError::WrongKind.into());
        }
        let old = self.decorations.get(&id).cloned().unwrap_or_default();
        let mut decoration = old.clone();
        change(&mut decoration);
        if (decoration.font_size, decoration.font) == (old.font_size, old.font) {
            return Ok(());
        }
        let mut style = face(self.theme_of(id), Some(&decoration));
        let control = &mut self.tree.get_mut(id).unwrap().context.control;
        control.text_role(&mut style);
        control.restyle(&mut self.fonts.borrow_mut(), &style)?;
        self.decorations.insert(id, decoration);
        self.trim_decoration(id);
        self.tree.mark_dirty(id, Dirty::ALL)?;
        self.ime_dirty = true;
        Ok(())
    }

    /// Marks what a change of interaction state invalidates. The control's
    /// kind may have changed with its state (a toggle), so a kind skin in
    /// scope is resolved again.
    pub fn dirty_visual_state(&mut self, id: NodeId) -> Result {
        if !self.skins.is_empty() {
            self.resolve_skin(id)?;
        }
        let dirty = if self.tree.get(id).unwrap().context.skin.is_some() {
            // A custom skin can change foreground as a function of any state.
            Dirty::PAINT | Dirty::SEMANTICS
        } else {
            Dirty::PAINT
        };
        self.tree.mark_dirty(id, dirty)?;
        Ok(())
    }
}

/// The text style under `theme` with a decoration's size and font.
fn face(theme: &Theme, decoration: Option<&Decoration>) -> TextStyle<'static> {
    let font = decoration.and_then(|d| d.font).unwrap_or_default();
    TextStyle {
        families: font.families,
        size: decoration
            .and_then(|d| d.font_size)
            .unwrap_or(theme.font_size),
        weight: FontWeight::new(f32::from(font.weight)),
        slant: match font.italic {
            true => FontStyle::Italic,
            false => FontStyle::Normal,
        },
        ..crate::state::text_style(theme)
    }
}
