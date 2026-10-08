//! Element contracts: what markup may write on a control a library provides.
//!
//! Every element, built-in or third-party, is described by one
//! [`ElementSpec`]. The checker reads only these specs, so a library's
//! elements are checked exactly like the default ones. Specs borrow their
//! strings so libraries declare them as constants.

use crate::{Type, Value};

/// How an element lays out its children; it decides the element's layout
/// properties and whether it has children at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// No children.
    Leaf,
    /// Children with gaps and alignment, but no direction of its own to set.
    Box,
    /// A flex container: adds `direction` and `wrap`.
    Flex,
    /// A grid container: adds tracks, areas and flow.
    Grid,
}

/// Which children a container element accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Children<'a> {
    /// Any controls, blocks and component instances.
    Any,
    /// Only elements of this name, written directly.
    Only(&'a str),
    /// Exactly this many elements, written directly.
    Exactly(u8),
}

/// Style groups an element accepts beyond the ones every control has; the
/// same groups as the control kind's `Accepts`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Styles(pub u8);

impl Styles {
    /// No optional style group.
    pub const NONE: Self = Self(0);
    /// `font_size`.
    pub const TEXT: Self = Self(1);
    /// `hover_background`, `focus_color` and `focus_width`.
    pub const INTERACTIVE: Self = Self(2);
    /// `pressed_background`.
    pub const PRESSED: Self = Self(4);
    /// `indicator_color`.
    pub const INDICATOR: Self = Self(8);
    /// `selection_color` and `caret_color`.
    pub const EDITOR: Self = Self(16);

    /// Both sets of groups.
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    /// Whether every group of `other` is accepted.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// The value an element property takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ValueType<'a> {
    /// `true` or `false`.
    Bool,
    /// A whole number in `min..=max`.
    Int(i64, i64),
    /// A finite number in `min..=max`.
    Float(f64, f64),
    /// A number from 0 to 1, or a percentage from 0% to 100%.
    Fraction,
    /// A finite nonnegative dp length.
    Length,
    /// Any string.
    String,
    /// A string without hard line separators.
    Line,
    /// A `#RRGGBB` or `#RRGGBBAA` color.
    Color,
    /// One of these bare identifiers.
    Choice(&'a [&'a str]),
}

/// One element property.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropertySpec<'a> {
    /// Name in markup.
    pub name: &'a str,
    /// Accepted values.
    pub ty: ValueType<'a>,
    /// A constructor argument: a literal value is passed when the control is
    /// created instead of being set afterwards.
    pub new: bool,
    /// Has a setter, so it may also be bound to an expression.
    pub set: bool,
    /// A constructor argument without a default; markup must write it as a literal.
    pub required: bool,
}

/// What markup may write on one element: its name, layout, style groups,
/// properties, events and the `self` fields its event handlers read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementSpec<'a> {
    /// Name in markup, such as `Button`.
    pub name: &'a str,
    /// Layout of its children.
    pub layout: Layout,
    /// Children a container accepts; ignored for a leaf.
    pub children: Children<'a>,
    /// The element it must be written directly inside, if any.
    pub parent: Option<&'a str>,
    /// Optional style groups.
    pub styles: Styles,
    /// Element properties, after the properties every node has.
    pub properties: &'a [PropertySpec<'a>],
    /// Events handled with `on name { ... }`; they carry no value.
    pub events: &'a [&'a str],
    /// Fields read as `self.name` inside its event handlers.
    pub fields: &'a [(&'a str, ValueType<'a>)],
}

impl ValueType<'_> {
    /// The expression type a property of this type may be bound to.
    pub fn binding(self) -> Option<Type> {
        Some(match self {
            Self::Bool => Type::Bool,
            Self::Int(..) => Type::Int,
            Self::Float(..) | Self::Fraction | Self::Length => Type::Float,
            Self::String | Self::Line => Type::String,
            Self::Color | Self::Choice(_) => return None,
        })
    }

    /// Checks a literal, with integers already read as numbers.
    pub(crate) fn validate(self, value: &Value) -> Result<(), String> {
        let valid = match (self, value) {
            (Self::Bool, Value::Bool(_)) | (Self::String, Value::String(_)) => true,
            (Self::Color, Value::Color(_)) => true,
            (Self::Int(min, max), Value::Number(n)) => {
                n.fract() == 0.0 && (min as f64..=max as f64).contains(&f64::from(*n))
            }
            (Self::Float(min, max), Value::Number(n)) => {
                n.is_finite() && (min..=max).contains(&f64::from(*n))
            }
            (Self::Fraction, Value::Number(n)) => (0.0..=1.0).contains(n),
            (Self::Fraction, Value::Percent(n)) => (0.0..=100.0).contains(n),
            (Self::Length, Value::Length(n)) => n.is_finite() && *n >= 0.0,
            (Self::Line, Value::String(text)) => !text.contains([
                '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
            ]),
            (Self::Choice(choices), Value::Identifier(name)) => choices.contains(&name.as_str()),
            _ => false,
        };
        if valid {
            return Ok(());
        }
        Err(match self {
            Self::Bool => "true or false".into(),
            Self::Int(min, max) => format!("a whole number from {min} to {max}"),
            Self::Float(min, max) if min == f64::MIN && max == f64::MAX => "a finite number".into(),
            Self::Float(min, max) if max == f64::MAX => {
                format!("a finite number of at least {min}")
            }
            Self::Float(min, max) => format!("a number from {min} to {max}"),
            Self::Fraction => "a number from 0 to 1 or a percentage".into(),
            Self::Length => "a nonnegative dp length".into(),
            Self::String => "a string".into(),
            Self::Line => "a string without hard line separators".into(),
            Self::Color => "a #RRGGBB or #RRGGBBAA color".into(),
            Self::Choice(choices) => choices.join(", "),
        })
    }
}

impl<'a> ElementSpec<'a> {
    /// The property named `name` and its index.
    pub fn property(&self, name: &str) -> Option<(usize, &PropertySpec<'a>)> {
        self.properties
            .iter()
            .enumerate()
            .find(|(_, property)| property.name == name)
    }
    /// Whether it has children.
    pub fn is_container(&self) -> bool {
        self.layout != Layout::Leaf
    }
}
