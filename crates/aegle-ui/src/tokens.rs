//! The token registry and per-UI token state: global and subtree overrides of
//! custom tokens, and properties bound to tokens.
//!
//! Names live in one registry per thread, shared by every [`crate::Ui`] on it,
//! so a component package registers its tokens once. Built-in tokens are the
//! [`Theme`] fields; their overrides are themes and theme overrides.

#[cfg(feature = "motion")]
use crate::TransitionProperty;
use crate::{Color, Insets, Result, State, Style, Theme, UiError};
use aegle_core::NodeId;
use aegle_theme::{BUILTIN_TOKENS, Token, TokenDefault, TokenKind, TokenType, TokenValue};
use std::{cell::RefCell, collections::HashMap};

struct Registry {
    entries: Vec<(TokenKind, Option<TokenDefault>)>,
    names: HashMap<Box<str>, u16>,
}

thread_local! {
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry {
        entries: BUILTIN_TOKENS.iter().map(|&(_, kind)| (kind, None)).collect(),
        names: BUILTIN_TOKENS
            .iter()
            .zip(0..)
            .map(|(&(name, _), index)| (name.into(), index))
            .collect(),
    });
}

/// Whether `name` is at least two dot-separated segments of ASCII letters,
/// digits, `_` and `-`, such as `editor.track_gap`.
fn valid_name(name: &str) -> bool {
    name.contains('.')
        && name.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

/// Registers a token under a namespaced name such as `"editor.lane"`, with a
/// default that follows the theme, and returns its handle. Registering a name
/// again with the same type returns the first handle and keeps the first
/// default. The registry is per thread and shared by its UIs.
///
/// Fails with [`UiError::InvalidValue`] for a malformed name or one in the
/// reserved `theme.` namespace, [`UiError::Token`] if the name has another
/// type, and [`UiError::IdentityExhausted`] past 65 536 tokens.
pub fn register_token<T: TokenType>(name: &str, default: fn(&Theme) -> T) -> Result<Token<T>> {
    if !valid_name(name) || name.starts_with("theme.") {
        return Err(UiError::InvalidValue.into());
    }
    REGISTRY.with_borrow_mut(|registry| {
        if let Some(&index) = registry.names.get(name) {
            return match registry.entries[usize::from(index)].0 == T::KIND {
                true => Ok(Token::from_index(index)),
                false => Err(UiError::Token.into()),
            };
        }
        let index =
            u16::try_from(registry.entries.len()).map_err(|_| UiError::IdentityExhausted)?;
        registry
            .entries
            .push((T::KIND, Some(T::default_fn(default))));
        registry.names.insert(name.into(), index);
        Ok(Token::from_index(index))
    })
}

/// The handle of a registered token, including built-ins such as
/// `"theme.accent"`; [`UiError::Token`] if unregistered or of another type.
pub fn token<T: TokenType>(name: &str) -> Result<Token<T>> {
    REGISTRY.with_borrow(|registry| match registry.names.get(name) {
        Some(&index) if registry.entries[usize::from(index)].0 == T::KIND => {
            Ok(Token::from_index(index))
        }
        _ => Err(UiError::Token.into()),
    })
}

/// Checks that a handle names a registered token of its type, and whether it
/// is built in.
pub(crate) fn check<T: TokenType>(token: Token<T>) -> Result<bool> {
    REGISTRY.with_borrow(
        |registry| match registry.entries.get(usize::from(token.index())) {
            Some(&(kind, default)) if kind == T::KIND => Ok(default.is_none()),
            _ => Err(UiError::Token.into()),
        },
    )
}

/// A color property that can follow a token: a [`Style`] color, or a stop of
/// the background gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum ColorSlot {
    Background,
    Foreground,
    HoverBackground,
    PressedBackground,
    DisabledBackground,
    DisabledForeground,
    BorderColor,
    FocusColor,
    Selection,
    Caret,
    Indicator,
    /// The color of stop `n` of `Node::set_background_gradient`; unbinding
    /// keeps its last color.
    GradientStop(u8),
}

/// A length property that can follow a token: the [`Style`] widths and
/// radius, the font size of a text-bearing control, or a uniform padding or
/// gap, as `Node::set_padding` and `Node::set_gap` set them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum LengthSlot {
    BorderWidth,
    Radius,
    FocusWidth,
    FontSize,
    Padding,
    Gap,
}

/// Any property that can follow a token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenSlot {
    /// A color property.
    Color(ColorSlot),
    /// A length property.
    Length(LengthSlot),
    /// The font face of a text-bearing control.
    Font,
    /// The shadow of `Node::set_shadow`.
    Shadow,
    /// The duration of a property transition.
    #[cfg(feature = "motion")]
    Transition(TransitionProperty),
}

impl From<ColorSlot> for TokenSlot {
    fn from(slot: ColorSlot) -> Self {
        Self::Color(slot)
    }
}

impl From<LengthSlot> for TokenSlot {
    fn from(slot: LengthSlot) -> Self {
        Self::Length(slot)
    }
}

impl TokenSlot {
    /// Whether the property is a [`Style`] field, which `set_style` replaces.
    pub(crate) fn is_style(self) -> bool {
        matches!(
            self,
            Self::Color(
                ColorSlot::Background
                    | ColorSlot::Foreground
                    | ColorSlot::HoverBackground
                    | ColorSlot::PressedBackground
                    | ColorSlot::DisabledBackground
                    | ColorSlot::DisabledForeground
                    | ColorSlot::BorderColor
                    | ColorSlot::FocusColor
                    | ColorSlot::Selection
                    | ColorSlot::Caret
                    | ColorSlot::Indicator
            ) | Self::Length(LengthSlot::BorderWidth | LengthSlot::Radius | LengthSlot::FocusWidth)
        )
    }

    /// Whether the property is a transition duration.
    #[cfg(feature = "motion")]
    pub(crate) fn is_transition(self) -> bool {
        matches!(self, Self::Transition(_))
    }
}

impl ColorSlot {
    pub(crate) fn field(self, style: &mut Style) -> &mut Option<Color> {
        match self {
            Self::Background => &mut style.background,
            Self::Foreground => &mut style.foreground,
            Self::HoverBackground => &mut style.hover_background,
            Self::PressedBackground => &mut style.pressed_background,
            Self::DisabledBackground => &mut style.disabled_background,
            Self::DisabledForeground => &mut style.disabled_foreground,
            Self::BorderColor => &mut style.border_color,
            Self::FocusColor => &mut style.focus_color,
            Self::Selection => &mut style.selection,
            Self::Caret => &mut style.caret,
            Self::Indicator => &mut style.indicator,
            Self::GradientStop(_) => unreachable!("not a style field"),
        }
    }
}

impl LengthSlot {
    fn field(self, style: &mut Style) -> &mut Option<f32> {
        match self {
            Self::BorderWidth => &mut style.border_width,
            Self::Radius => &mut style.radius,
            Self::FocusWidth => &mut style.focus_width,
            Self::FontSize | Self::Padding | Self::Gap => unreachable!("not a style field"),
        }
    }
}

/// Custom token overrides and bindings of one UI.
#[derive(Default)]
pub struct Tokens {
    /// Overrides for the whole UI.
    global: HashMap<u16, TokenValue>,
    /// Overrides for a subtree; the nearest ancestor's wins.
    local: HashMap<NodeId, Vec<(u16, TokenValue)>>,
    /// Properties following a token, re-resolved whenever it may change.
    bindings: HashMap<NodeId, Vec<(TokenSlot, u16)>>,
}

impl Tokens {
    /// Forgets a removed node.
    pub fn forget(&mut self, id: NodeId) {
        self.local.remove(&id);
        self.bindings.remove(&id);
    }

    /// Whether a property of `id` follows a token.
    pub fn is_bound(&self, id: NodeId, slot: TokenSlot) -> bool {
        self.bindings
            .get(&id)
            .is_some_and(|bound| bound.iter().any(|&(s, _)| s == slot))
    }

    /// Removes a binding, leaving its property's current value.
    pub fn unbind(&mut self, id: NodeId, slot: impl Fn(TokenSlot) -> bool) {
        if let Some(bound) = self.bindings.get_mut(&id) {
            bound.retain(|&(s, _)| !slot(s));
            if bound.is_empty() {
                self.bindings.remove(&id);
            }
        }
    }
}

/// A token value is valid and, for a length, nonnegative.
fn valid(value: TokenValue) -> Result<TokenValue> {
    match value {
        TokenValue::Length(v) if v < 0.0 => Err(UiError::InvalidValue.into()),
        value if !value.is_valid() => Err(UiError::InvalidValue.into()),
        value => Ok(value),
    }
}

impl State {
    /// The value of token `index` at `id`: built-ins from the resolved theme,
    /// custom tokens from the nearest subtree override, the UI override or
    /// the default for the resolved theme.
    pub fn token_value(&self, id: NodeId, index: u16) -> Result<TokenValue> {
        let theme = self.theme_of(id);
        if let Some(value) = theme.token(index) {
            return Ok(value);
        }
        let mut node = Some(id);
        while let Some(n) = node {
            let local = self.tokens.local.get(&n);
            if let Some(&(_, value)) = local.and_then(|l| l.iter().find(|(i, _)| *i == index)) {
                return Ok(value);
            }
            node = self.tree.parent(n)?;
        }
        if let Some(&value) = self.tokens.global.get(&index) {
            return Ok(value);
        }
        let default = REGISTRY.with_borrow(|r| r.entries.get(usize::from(index)).and_then(|e| e.1));
        valid(default.ok_or(UiError::Token)?.resolve(theme))
    }

    /// Sets or clears a custom token for the UI (`id` is `None`) or a subtree,
    /// re-resolving bindings; on failure the previous value is restored.
    pub fn set_custom_token(
        &mut self,
        id: Option<NodeId>,
        index: u16,
        value: Option<TokenValue>,
    ) -> Result {
        let value = value.map(valid).transpose()?;
        let old = self.replace_token(id, index, value);
        let outcome = self.refresh_tokens(id);
        if outcome.is_err() {
            self.replace_token(id, index, old);
            self.refresh_tokens(id)?;
        }
        outcome
    }

    fn replace_token(
        &mut self,
        id: Option<NodeId>,
        index: u16,
        value: Option<TokenValue>,
    ) -> Option<TokenValue> {
        let Some(id) = id else {
            return match value {
                Some(value) => self.tokens.global.insert(index, value),
                None => self.tokens.global.remove(&index),
            };
        };
        let local = self.tokens.local.entry(id).or_default();
        let old = local
            .iter()
            .position(|(i, _)| *i == index)
            .map(|at| local.swap_remove(at).1);
        match value {
            Some(value) => local.push((index, value)),
            None if local.is_empty() => {
                self.tokens.local.remove(&id);
            }
            None => {}
        }
        old
    }

    /// Binds a property to token `index` and applies its current value; on
    /// failure, an earlier binding of the property stays.
    pub fn bind_token(&mut self, id: NodeId, slot: TokenSlot, index: u16) -> Result {
        self.apply_token(id, slot, index)?;
        self.tokens.unbind(id, |s| s == slot);
        self.tokens
            .bindings
            .entry(id)
            .or_default()
            .push((slot, index));
        Ok(())
    }

    fn apply_token(&mut self, id: NodeId, slot: TokenSlot, index: u16) -> Result {
        let value = self.token_value(id, index)?;
        self.write_slot(id, slot, Some(value))
    }

    /// Sets a bindable property, or with `None` clears it, returning it to the
    /// skin or theme. The value has the slot's kind.
    pub fn write_slot(&mut self, id: NodeId, slot: TokenSlot, value: Option<TokenValue>) -> Result {
        fn typed<T: TokenType>(value: Option<TokenValue>) -> Result<Option<T>> {
            value
                .map(|v| T::from_value(v).ok_or_else(|| UiError::Token.into()))
                .transpose()
        }
        match slot {
            TokenSlot::Color(ColorSlot::GradientStop(index)) => match typed(value)? {
                Some(color) => self.set_gradient_stop(id, index, color),
                None => Ok(()),
            },
            TokenSlot::Color(slot) => {
                let color = typed(value)?;
                self.edit_style(id, |style| *slot.field(style) = color)
            }
            TokenSlot::Length(LengthSlot::FontSize) => self.set_font_size(id, typed(value)?),
            TokenSlot::Length(LengthSlot::Padding) => {
                self.set_padding(id, typed::<f32>(value)?.map(Insets::all))
            }
            TokenSlot::Length(LengthSlot::Gap) => {
                self.set_gap(id, typed::<f32>(value)?.map(|g| (g.into(), g.into())))
            }
            TokenSlot::Length(slot) => {
                let length = typed(value)?;
                self.edit_style(id, |style| *slot.field(style) = length)
            }
            TokenSlot::Font => self.set_font(id, typed(value)?),
            TokenSlot::Shadow => self.transition_shadow(id, typed(value)?),
            #[cfg(feature = "motion")]
            TokenSlot::Transition(property) => {
                let easing = self
                    .motion
                    .tracks
                    .get(&id)
                    .and_then(|track| track.timing(property))
                    .map_or(crate::Transition::default().easing, |timing| timing.easing);
                let timing = typed(value)?.map(|duration| crate::Transition::new(duration, easing));
                self.set_property_transition(id, property, timing)
            }
        }
    }

    /// Re-resolves bindings in the subtree of `scope`, or everywhere.
    pub fn refresh_tokens(&mut self, scope: Option<NodeId>) -> Result {
        let bound: Vec<_> = self
            .tokens
            .bindings
            .iter()
            .filter(|(id, _)| scope.is_none_or(|root| self.contains(root, **id)))
            .flat_map(|(&id, slots)| slots.iter().map(move |&(slot, index)| (id, slot, index)))
            .collect();
        bound
            .into_iter()
            .try_for_each(|(id, slot, index)| self.apply_token(id, slot, index))
    }
}
