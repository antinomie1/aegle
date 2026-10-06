use crate::{
    Appearance, Color, Node, Point, Result, Skin, Style, UiError, VisualState,
    tokens::{ColorSlot, LengthSlot, TokenSlot},
};
use aegle_core::Dirty;
use aegle_theme::Font;

macro_rules! setters {
    ($(#[$doc:meta] $name:ident($value:ident: $ty:ty) => $field:ident, $slot:expr;)*) => {
        $(#[$doc]
        /// Ends a token binding of the property.
        pub fn $name(&self, $value: $ty) -> Result {
            self.update_style($slot.into(), |style| style.$field = Some($value))
        })*
    };
}

impl Node {
    /// Replaces local paint overrides, preserving the skin and typography.
    /// `Style::default()` removes all paint overrides. Values do not inherit.
    /// Token bindings of style properties end; font and layout bindings remain.
    /// Hover/focus overrides require an interactive control, pressed requires a
    /// button/toggle/slider, and selection/caret require an editor; otherwise returns WrongKind.
    pub fn set_style(&self, style: Style) -> Result {
        self.change(|state, id| {
            state.set_style(id, style)?;
            state.tokens.unbind(id, TokenSlot::is_style);
            Ok(())
        })
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
    fn update_style(&self, slot: TokenSlot, update: impl FnOnce(&mut Style)) -> Result {
        self.write_unbound(slot, |state, id| state.edit_style(id, update))
    }
    /// Sets a bindable property through `write`, then ends its binding.
    fn write_unbound(
        &self,
        slot: TokenSlot,
        write: impl FnOnce(&mut crate::State, aegle_core::NodeId) -> Result,
    ) -> Result {
        self.change(|state, id| {
            write(state, id)?;
            state.tokens.unbind(id, |s| s == slot);
            Ok(())
        })
    }
    /// Installs a pure theme/state skin without changing control behavior.
    /// The current result is validated before storing it; later states are
    /// validated during refresh. Local paint overrides keep their precedence.
    pub fn set_skin(&self, skin: Skin) -> Result {
        self.change(|state, id| {
            skin(state.theme_of(id), state.visual_state(id)).validate()?;
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
        self.change(|state, id| state.appearance(id))
    }
    /// Reads effective enabled, focus, pointer and editor policy state for a skin.
    /// Hover/focus are available on interactive controls; pressed on buttons,
    /// toggles and sliders. Checked is meaningful for toggles, read-only for editors.
    pub fn visual_state(&self) -> Result<VisualState> {
        self.change(|state, id| Ok(state.visual_state(id)))
    }
    /// Sets a positive finite local text size, retaining text, selection and preedit.
    /// Available on labels, buttons, toggles and editors; it does not inherit to
    /// children. Ends a font size token binding.
    pub fn set_font_size(&self, size: f32) -> Result {
        let slot = LengthSlot::FontSize.into();
        self.write_unbound(slot, |state, id| state.set_font_size(id, Some(size)))
    }
    /// Returns this text-bearing control to the current theme's font size,
    /// ending a font size token binding.
    pub fn clear_font_size(&self) -> Result {
        let slot = LengthSlot::FontSize.into();
        self.write_unbound(slot, |state, id| state.set_font_size(id, None))
    }
    /// Sets the font face of a label, button, toggle or editor, reshaping
    /// its text and keeping selection and preedit; it does not inherit to
    /// children. Ends a font token binding. Fails with InvalidValue for blank
    /// families or a weight outside 1–1000.
    pub fn set_font(&self, font: Font) -> Result {
        self.write_unbound(TokenSlot::Font, |state, id| state.set_font(id, Some(font)))
    }
    /// Returns this text-bearing control to [`Font::DEFAULT`], ending a font
    /// token binding.
    pub fn clear_font(&self) -> Result {
        self.write_unbound(TokenSlot::Font, |state, id| state.set_font(id, None))
    }
    /// The local font face; `None` uses [`Font::DEFAULT`].
    pub fn font(&self) -> Result<Option<Font>> {
        self.change(|state, id| Ok(state.decorations.get(&id).and_then(|d| d.font)))
    }
    /// Translates this subtree by a finite logical offset after layout, without
    /// changing layout or scroll extents. Bounds, hit testing, clipping, the IME
    /// anchor and accessibility follow it. With `motion`, a control with a
    /// transition policy animates from its presented offset.
    pub fn set_offset(&self, offset: Point) -> Result {
        if !(offset.x.is_finite() && offset.y.is_finite()) {
            return Err(UiError::InvalidValue.into());
        }
        self.change(|state, id| {
            #[cfg(feature = "motion")]
            return state.transition_offset(id, offset);
            #[cfg(not(feature = "motion"))]
            {
                state.tree.get_mut(id).unwrap().context.offset = offset;
                state.geometry_dirty = true;
                state.repaint = true;
                Ok(())
            }
        })
    }
    /// The logical target offset; [`Self::bounds`] reflects the presented one.
    pub fn offset(&self) -> Result<Point> {
        self.change(|state, id| {
            #[cfg(feature = "motion")]
            if let Some(active) = state.motion.moving.get(&id) {
                return Ok(active.tween.target());
            }
            Ok(state.tree.get(id).unwrap().context.offset)
        })
    }
    setters! {
        /// Sets the base background, taking precedence over the skin.
        set_background(color: Color) => background, ColorSlot::Background;
        /// Sets the text foreground without reshaping or changing layout.
        set_foreground(color: Color) => foreground, ColorSlot::Foreground;
        /// Sets the background while enabled, hovered and not pressed.
        set_hover_background(color: Color) => hover_background, ColorSlot::HoverBackground;
        /// Sets the background while enabled and pressed.
        set_pressed_background(color: Color) => pressed_background, ColorSlot::PressedBackground;
        /// Sets the background while effectively disabled.
        set_disabled_background(color: Color) => disabled_background, ColorSlot::DisabledBackground;
        /// Sets the text foreground while effectively disabled.
        set_disabled_foreground(color: Color) => disabled_foreground, ColorSlot::DisabledForeground;
        /// Sets the independent resting border color.
        set_border_color(color: Color) => border_color, ColorSlot::BorderColor;
        /// Sets a nonnegative logical border width; zero removes the border.
        set_border_width(width: f32) => border_width, LengthSlot::BorderWidth;
        /// Sets a nonnegative logical corner radius.
        set_radius(radius: f32) => radius, LengthSlot::Radius;
        /// Sets the independent focus outline color.
        set_focus_color(color: Color) => focus_color, ColorSlot::FocusColor;
        /// Sets nonnegative focus width; its target is zero when disabled or unfocused.
        set_focus_width(width: f32) => focus_width, LengthSlot::FocusWidth;
        /// Sets an editor's selection fill, paired with its text foreground.
        set_selection_color(color: Color) => selection, ColorSlot::Selection;
        /// Sets an editor's caret and preedit indicator color.
        set_caret_color(color: Color) => caret, ColorSlot::Caret;
        /// Sets checkbox/switch marks or slider/progress indicator colors.
        set_indicator_color(color: Color) => indicator, ColorSlot::Indicator;
    }
}
