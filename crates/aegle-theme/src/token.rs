//! Typed theme tokens: compact handles to named values that a host registers
//! once, by namespaced name, and resolves against a theme. Controls and styles
//! hold indices, never strings.

use core::{fmt, hash, marker::PhantomData, time::Duration};

use aegle_types::Color;

use crate::{Theme, ThemeOverride};

/// The kinds of value a token holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// An unpremultiplied sRGB color.
    Color,
    /// Logical pixels, finite.
    Length,
    /// A time span, such as a transition length.
    Duration,
}

/// A token value of any kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenValue {
    /// A color.
    Color(Color),
    /// Finite logical pixels.
    Length(f32),
    /// A time span.
    Duration(Duration),
}

impl TokenValue {
    /// The kind of this value.
    pub fn kind(self) -> TokenKind {
        match self {
            Self::Color(_) => TokenKind::Color,
            Self::Length(_) => TokenKind::Length,
            Self::Duration(_) => TokenKind::Duration,
        }
    }

    /// Whether a length is finite; colors and durations are always valid.
    pub fn is_valid(self) -> bool {
        !matches!(self, Self::Length(v) if !v.is_finite())
    }
}

/// How a token's value follows the theme when nothing overrides it: a pure
/// function, like a [`crate::Skin`], so dark and light themes can differ.
#[derive(Clone, Copy, Debug)]
pub enum TokenDefault {
    /// A color from the theme.
    Color(fn(&Theme) -> Color),
    /// Logical pixels from the theme.
    Length(fn(&Theme) -> f32),
    /// A time span from the theme.
    Duration(fn(&Theme) -> Duration),
}

impl TokenDefault {
    /// The value for `theme`.
    pub fn resolve(self, theme: &Theme) -> TokenValue {
        match self {
            Self::Color(f) => TokenValue::Color(f(theme)),
            Self::Length(f) => TokenValue::Length(f(theme)),
            Self::Duration(f) => TokenValue::Duration(f(theme)),
        }
    }

    /// The kind of value it gives.
    pub fn kind(self) -> TokenKind {
        match self {
            Self::Color(_) => TokenKind::Color,
            Self::Length(_) => TokenKind::Length,
            Self::Duration(_) => TokenKind::Duration,
        }
    }
}

/// A Rust type a token can hold: [`Color`], `f32` logical pixels or [`Duration`].
pub trait TokenType: Copy + 'static {
    /// The kind of token holding this type.
    const KIND: TokenKind;
    /// As a value of any kind.
    fn into_value(self) -> TokenValue;
    /// From a value of this type's kind.
    fn from_value(value: TokenValue) -> Option<Self>;
    /// A default function as the kind-erased form a registry stores.
    fn default_fn(default: fn(&Theme) -> Self) -> TokenDefault;
}

macro_rules! token_type {
    ($ty:ty, $kind:ident) => {
        impl TokenType for $ty {
            const KIND: TokenKind = TokenKind::$kind;
            fn into_value(self) -> TokenValue {
                TokenValue::$kind(self)
            }
            fn from_value(value: TokenValue) -> Option<Self> {
                match value {
                    TokenValue::$kind(v) => Some(v),
                    _ => None,
                }
            }
            fn default_fn(default: fn(&Theme) -> Self) -> TokenDefault {
                TokenDefault::$kind(default)
            }
        }
    };
}
token_type!(Color, Color);
token_type!(f32, Length);
token_type!(Duration, Duration);

/// A typed handle to a registered token: its index in the registry, which
/// stays valid for the registry's lifetime. The first indices are the
/// built-in [`Theme`] tokens, such as [`Theme::ACCENT`].
pub struct Token<T> {
    index: u16,
    kind: PhantomData<fn() -> T>,
}

impl<T> Token<T> {
    /// The handle for a registry index. A registry checks the index and kind
    /// whenever a handle reaches it, so a forged one is rejected, not misread.
    pub const fn from_index(index: u16) -> Self {
        Self {
            index,
            kind: PhantomData,
        }
    }

    /// The registry index.
    pub const fn index(self) -> u16 {
        self.index
    }
}

impl<T> Clone for Token<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Token<T> {}
impl<T> PartialEq for Token<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
    }
}
impl<T> Eq for Token<T> {}
impl<T> hash::Hash for Token<T> {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
    }
}
impl<T> fmt::Debug for Token<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({})", self.index)
    }
}

macro_rules! builtin {
    ($($index:literal $constant:ident $field:ident $kind:ident $ty:ty;)*) => {
        /// Names and kinds of the built-in tokens, in index order: one per
        /// [`Theme`] field, named `theme.<field>`.
        pub const BUILTIN_TOKENS: &[(&str, TokenKind)] =
            &[$((concat!("theme.", stringify!($field)), TokenKind::$kind)),*];

        impl Theme {
            $(
                #[doc = concat!("The built-in token for [`Theme::", stringify!($field), "`].")]
                pub const $constant: Token<$ty> = Token::from_index($index);
            )*

            /// A built-in token's value; `None` past the built-in indices.
            pub fn token(&self, index: u16) -> Option<TokenValue> {
                Some(match index {
                    $($index => self.$field.into_value(),)*
                    _ => return None,
                })
            }

            /// This theme with a built-in token replaced, or `None` for a
            /// custom index or a value of another kind. Not validated.
            pub fn with_token(mut self, index: u16, value: TokenValue) -> Option<Self> {
                match index {
                    $($index => self.$field = <$ty>::from_value(value)?,)*
                    _ => return None,
                }
                Some(self)
            }
        }

        impl ThemeOverride {
            /// Sets or clears the replacement for a built-in token; `false`
            /// for a custom index or a value of another kind.
            pub fn set_token(&mut self, index: u16, value: Option<TokenValue>) -> bool {
                match (index, value) {
                    $(
                        ($index, None) => self.$field = None,
                        ($index, Some(value)) => match <$ty>::from_value(value) {
                            Some(value) => self.$field = Some(value),
                            None => return false,
                        },
                    )*
                    _ => return false,
                }
                true
            }
        }
    };
}

builtin! {
    0 BACKGROUND background Color Color;
    1 SURFACE surface Color Color;
    2 FOREGROUND foreground Color Color;
    3 MUTED muted Color Color;
    4 ACCENT accent Color Color;
    5 BORDER border Color Color;
    6 HOVER hover Color Color;
    7 PRESSED pressed Color Color;
    8 SELECTION selection Color Color;
    9 FONT_SIZE font_size Length f32;
    10 PADDING padding Length f32;
    11 GAP gap Length f32;
    12 RADIUS radius Length f32;
    13 CONTROL_HEIGHT control_height Length f32;
}
