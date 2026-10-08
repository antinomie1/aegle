use crate::{
    Appearance, Color, Node, Point, Result, Skin, State, Style, UiError, VisualState,
    tokens::{ColorSlot, LengthSlot, TokenSlot},
};
use aegle_core::{Dirty, NodeId};

impl Node {
    /// Replaces every local paint override at once, preserving the skin and
    /// typography; the one-field setters are shorthands for its fields.
    /// `Style::default()` removes all overrides. Values do not inherit.
    /// Writing every field, it ends every style token binding; font and
    /// layout bindings remain. Hover/focus fields require an interactive
    /// control, pressed a button/toggle/slider, indicator a toggle, slider or
    /// progress bar, and selection/caret an editor; otherwise returns WrongKind.
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
    crate::style_setters! {
        /// Sets the base background, taking precedence over the skin.
        set_background(color: Color) => background, ColorSlot::Background;
        /// Sets the text foreground without reshaping or changing layout.
        set_foreground(color: Color) => foreground, ColorSlot::Foreground;
        /// Sets the background while effectively disabled.
        set_disabled_background(color: Color) => disabled_background, ColorSlot::DisabledBackground;
        /// Sets the text foreground while effectively disabled.
        set_disabled_foreground(color: Color) => disabled_foreground, ColorSlot::DisabledForeground;
        /// Sets the resting border color.
        set_border_color(color: Color) => border_color, ColorSlot::BorderColor;
        /// Sets a nonnegative logical border width; zero removes the border.
        set_border_width(width: f32) => border_width, LengthSlot::BorderWidth;
        /// Sets a nonnegative logical corner radius.
        set_radius(radius: f32) => radius, LengthSlot::Radius;
    }
}

impl State {
    /// Edits local style fields like [`Node::set_style`], ending the token
    /// binding of `slot` only; the body of every one-field style setter.
    pub fn set_style_field(
        &mut self,
        id: NodeId,
        slot: TokenSlot,
        edit: impl FnOnce(&mut Style),
    ) -> Result {
        self.write_unbound(id, slot, |state| state.edit_style(id, edit))
    }
    /// Writes a bindable property of `id` through `write`, then ends the
    /// binding of `slot`: a direct write replaces a token binding.
    pub fn write_unbound(
        &mut self,
        id: NodeId,
        slot: TokenSlot,
        write: impl FnOnce(&mut Self) -> Result,
    ) -> Result {
        write(self)?;
        self.tokens.unbind(id, |s| s == slot);
        Ok(())
    }
}
