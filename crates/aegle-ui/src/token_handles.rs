use crate::{
    Color, Node, Result, Shadow, Ui, UiError,
    tokens::{ColorSlot, LengthSlot, TokenSlot, check},
};
use aegle_theme::{Font, Token, TokenType};

impl Ui {
    /// Overrides a token for the whole UI, or with `None` returns a custom
    /// token to its default. A built-in token replaces that field of the UI
    /// theme, as [`Self::set_theme`] would, and cannot be cleared. Bound
    /// properties follow; if one rejects the new value, the change is undone
    /// and its error returned.
    pub fn set_token<T: TokenType>(&self, token: Token<T>, value: impl Into<Option<T>>) -> Result {
        let value = value.into();
        if check(token)? {
            let value = value.ok_or(UiError::InvalidValue)?.into_value();
            let theme = self.theme()?.with_token(token.index(), value).unwrap();
            return self.set_theme(theme);
        }
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| UiError::ReentrantAccess)?;
        state.set_custom_token(None, token.index(), value.map(T::into_value))
    }

    /// A token's value for the UI: its override, or its default for the UI
    /// theme, ignoring subtree overrides.
    pub fn token_value<T: TokenType>(&self, token: Token<T>) -> Result<T> {
        check(token)?;
        let state = self.read()?;
        let value = match state.theme.token(token.index()) {
            Some(value) => value,
            None => state.token_value(state.root, token.index())?,
        };
        Ok(T::from_value(value).unwrap())
    }
}

impl Node {
    /// Overrides a token for this subtree, or with `None` removes the
    /// override. The nearest override wins over the UI's; local themes do not
    /// hide custom tokens. A built-in token is a sparse
    /// [`Self::set_theme_override`] entry, which replaces a local theme
    /// snapshot. Bound properties follow, as for [`Ui::set_token`].
    pub fn set_token<T: TokenType>(&self, token: Token<T>, value: impl Into<Option<T>>) -> Result {
        let value = value.into();
        let value = value.map(T::into_value);
        if check(token)? {
            let mut tokens = self.change(|state, id| Ok(state.overrides.get(&id).copied()))?;
            let set = tokens
                .get_or_insert_default()
                .set_token(token.index(), value);
            debug_assert!(set, "built-in tokens have the kind of their field");
            return self.set_theme_override(tokens.filter(|t| *t != Default::default()));
        }
        self.change(|state, id| state.set_custom_token(Some(id), token.index(), value))
    }

    /// A token's value here: built-ins from the resolved theme, custom tokens
    /// from the nearest override or their default for the resolved theme.
    pub fn token_value<T: TokenType>(&self, token: Token<T>) -> Result<T> {
        check(token)?;
        self.change(|state, id| Ok(T::from_value(state.token_value(id, token.index())?).unwrap()))
    }

    /// Makes a color property follow a token through theme, override and
    /// parent changes, applying it now. A direct setter for the property,
    /// [`Self::set_style`] or [`Self::unbind_token`] ends the binding.
    /// Fails like the property's setter if this control has no such property.
    pub fn bind_color(&self, slot: ColorSlot, token: Token<Color>) -> Result {
        check(token)?;
        self.change(|state, id| state.bind_token(id, slot.into(), token.index()))
    }

    /// [`Self::bind_color`] for a length: a style width or radius, the font
    /// size of a text-bearing control, which must stay positive, or a uniform
    /// padding or gap. `set_style` ends only style bindings.
    pub fn bind_length(&self, slot: LengthSlot, token: Token<f32>) -> Result {
        check(token)?;
        self.change(|state, id| state.bind_token(id, slot.into(), token.index()))
    }

    /// [`Self::bind_color`] for the font face of a text-bearing control;
    /// a handle's `set_font` ends it.
    pub fn bind_font(&self, token: Token<Font>) -> Result {
        check(token)?;
        self.change(|state, id| state.bind_token(id, TokenSlot::Font, token.index()))
    }

    /// [`Self::bind_color`] for the shadow, which tweens like
    /// [`Self::set_shadow`]; that setter ends the binding.
    pub fn bind_shadow(&self, token: Token<Shadow>) -> Result {
        check(token)?;
        self.change(|state, id| state.bind_token(id, TokenSlot::Shadow, token.index()))
    }

    /// Animates `property` with `easing` for the duration a token gives,
    /// following it like [`Self::bind_color`]. A transition setter for the
    /// property ends the binding.
    #[cfg(feature = "motion")]
    pub fn bind_transition(
        &self,
        property: crate::TransitionProperty,
        token: Token<std::time::Duration>,
        easing: crate::Easing,
    ) -> Result {
        check(token)?;
        self.change(|state, id| {
            let duration = state.token_value(id, token.index())?;
            let duration = TokenType::from_value(duration).unwrap();
            let timing = crate::Transition::new(duration, easing);
            state.set_property_transition(id, property, Some(timing))?;
            state.bind_token(id, TokenSlot::Transition(property), token.index())
        })
    }

    /// Ends a binding and clears the property, returning it to the skin or
    /// theme; a transition becomes immediate. Nothing happens if the property
    /// is not bound.
    pub fn unbind_token(&self, slot: impl Into<TokenSlot>) -> Result {
        let slot = slot.into();
        self.change(|state, id| {
            if !state.tokens.is_bound(id, slot) {
                return Ok(());
            }
            state.tokens.unbind(id, |s| s == slot);
            state.write_slot(id, slot, None)
        })
    }
}
