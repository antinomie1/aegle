use crate::{Appearance, Color, Node, Result, Skin, Style, VisualState};
use aegle_core::Dirty;

macro_rules! setters {
    ($(#[$doc:meta] $name:ident($value:ident: $ty:ty) => $field:ident;)*) => {
        $(#[$doc]
        pub fn $name(&self, $value: $ty) -> Result {
            self.update_style(|style| style.$field = Some($value))
        })*
    };
}

impl Node {
    /// Replaces local paint overrides, preserving the skin and typography.
    /// `Style::default()` removes all paint overrides. Values do not inherit.
    /// Hover/focus overrides require a button or editor, pressed requires a
    /// button, and selection/caret require an editor; otherwise returns WrongKind.
    pub fn set_style(&self, style: Style) -> Result {
        self.change(|state, id| state.set_style(id, style))
    }
    /// Reads local paint overrides; unresolved `None` values come from the skin.
    pub fn style(&self) -> Result<Style> {
        self.change(|state, id| {
            Ok(state
                .decorations
                .get(&id)
                .map_or(Style::default(), |d| d.style))
        })
    }
    fn update_style(&self, update: impl FnOnce(&mut Style)) -> Result {
        self.change(|state, id| {
            let mut style = state
                .decorations
                .get(&id)
                .map_or(Style::default(), |d| d.style);
            update(&mut style);
            state.set_style(id, style)
        })
    }
    /// Installs a pure theme/state skin without changing control behavior.
    /// The current result is validated before storing it; later states are
    /// validated during refresh. Local paint overrides keep their precedence.
    pub fn set_skin(&self, skin: Skin) -> Result {
        self.change(|state, id| {
            skin(&state.theme, state.visual_state(id)).validate()?;
            state.decorations.entry(id).or_default().skin = Some(skin);
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            Ok(())
        })
    }
    /// Restores the neutral skin, retaining explicit local overrides.
    pub fn clear_skin(&self) -> Result {
        self.change(|state, id| {
            if let Some(decoration) = state.decorations.get_mut(&id) {
                decoration.skin = None;
                state.trim_decoration(id);
                state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
            }
            Ok(())
        })
    }
    /// Resolves and validates the visible skin and local overrides in the current state.
    pub fn appearance(&self) -> Result<Appearance> {
        self.change(|state, id| {
            let appearance = state.appearance(id);
            appearance.validate()?;
            Ok(appearance)
        })
    }
    /// Reads effective enabled, focus, pointer and editor policy state for a skin.
    /// Hover/focus are available on buttons/editors, pressed on buttons; other
    /// controls report false. Read-only is meaningful only for editors.
    pub fn visual_state(&self) -> Result<VisualState> {
        self.change(|state, id| Ok(state.visual_state(id)))
    }
    /// Sets a positive finite local text size, retaining text, selection and preedit.
    /// Available on labels, buttons and editors; it does not inherit to children.
    pub fn set_font_size(&self, size: f32) -> Result {
        self.change(|state, id| state.set_font_size(id, Some(size)))
    }
    /// Returns this text-bearing control to the current theme's font size.
    pub fn clear_font_size(&self) -> Result {
        self.change(|state, id| state.set_font_size(id, None))
    }
    setters! {
        /// Sets the base background, taking precedence over the skin.
        set_background(color: Color) => background;
        /// Sets the text foreground without reshaping or changing layout.
        set_foreground(color: Color) => foreground;
        /// Sets the background while enabled, hovered and not pressed.
        set_hover_background(color: Color) => hover_background;
        /// Sets the background while enabled and pressed.
        set_pressed_background(color: Color) => pressed_background;
        /// Sets the background while effectively disabled.
        set_disabled_background(color: Color) => disabled_background;
        /// Sets the text foreground while effectively disabled.
        set_disabled_foreground(color: Color) => disabled_foreground;
        /// Sets the independent resting border color.
        set_border_color(color: Color) => border_color;
        /// Sets a nonnegative logical border width; zero removes the border.
        set_border_width(width: f32) => border_width;
        /// Sets a nonnegative logical corner radius.
        set_radius(radius: f32) => radius;
        /// Sets the independent focus outline color.
        set_focus_color(color: Color) => focus_color;
        /// Sets nonnegative focus outline width; drawn only while enabled and focused.
        set_focus_width(width: f32) => focus_width;
        /// Sets an editor's selection fill, paired with its text foreground.
        set_selection_color(color: Color) => selection;
        /// Sets an editor's caret and preedit indicator color.
        set_caret_color(color: Color) => caret;
    }
}
