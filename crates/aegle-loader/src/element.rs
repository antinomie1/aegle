//! The element contract: how markup creates and drives one kind of control.

use std::{any::Any, marker::PhantomData, rc::Rc};

use aegle_markup::{ElementSpec, Value, ValueType};
use aegle_ui::{Color, Container, Node, Result};

use crate::{Data, Handle};

/// A markup element: the spec markup is checked against, and the glue that
/// creates its control and applies properties, events and `self` fields.
///
/// Implement it with [`element!`](crate::element), never by hand. Indices
/// count the spec's properties, events and fields; the checker passes only
/// valid ones, with values of the declared types.
pub trait Element: 'static {
    /// The typed handle views expose for an `id` on this element.
    type Handle: Clone + 'static;
    /// What markup may write on this element.
    const SPEC: ElementSpec<'static>;
    /// Creates the control as the last child of `parent`, with the literal
    /// constructor arguments markup wrote, by property index.
    fn create(parent: &Container, args: &[(usize, Arg<'_>)]) -> Result<Self::Handle>;
    /// The control's node.
    fn node(handle: &Self::Handle) -> &Node;
    /// Sets the property with this index.
    fn set(handle: &Self::Handle, property: usize, value: Arg<'_>) -> Result;
    /// Reads the `self` field with this index.
    fn get(handle: &Self::Handle, field: usize) -> Result<Data>;
    /// Runs `run` after each occurrence of the event with this index.
    fn listen(handle: &Self::Handle, event: usize, run: Box<dyn Fn() -> Result>) -> Result;
    /// The container child number `child` is created in.
    fn parent(handle: &Self::Handle, child: usize) -> Container;
}

/// A literal or bound value on its way to an element's constructor or setter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Arg<'a> {
    /// `bool`.
    Bool(bool),
    /// `int`.
    Int(i64),
    /// `float`, `fraction` (0 to 1) or `length` (dp).
    Float(f64),
    /// `string`, `line` or a `choice` identifier.
    Str(&'a str),
    /// `color`.
    Color(Color),
}

macro_rules! read {
    ($($name:ident -> $ty:ty: $($pattern:pat => $value:expr),+;)*) => {
        impl<'a> Arg<'a> {
            $(
                #[doc = concat!("The checked `", stringify!($ty), "` value.")]
                pub fn $name(self) -> $ty {
                    match self {
                        $($pattern => $value,)+
                        _ => unreachable!("checked element value"),
                    }
                }
            )*
        }
    };
}

read! {
    bool -> bool: Arg::Bool(value) => value;
    i64 -> i64: Arg::Int(value) => value;
    f64 -> f64: Arg::Float(value) => value;
    f32 -> f32: Arg::Float(value) => value as f32;
    str -> &'a str: Arg::Str(value) => value;
    color -> Color: Arg::Color(value) => value;
}

impl<'a> Arg<'a> {
    /// A checked literal of type `ty`; integers arrive as numbers.
    pub(crate) fn literal(ty: ValueType<'_>, value: &'a Value) -> Self {
        match (ty, value) {
            (_, Value::Bool(value)) => Self::Bool(*value),
            (ValueType::Int(..), Value::Number(value)) => Self::Int(*value as i64),
            (_, Value::Number(value) | Value::Length(value)) => Self::Float(f64::from(*value)),
            (_, Value::Percent(value)) => Self::Float(f64::from(*value) / 100.0),
            (_, Value::String(value) | Value::Identifier(value)) => Self::Str(value),
            (_, Value::Color([r, g, b, a])) => Self::Color(Color::rgba(*r, *g, *b, *a)),
            _ => unreachable!("checked element literal"),
        }
    }

    /// The value of a checked binding.
    pub(crate) fn data(value: &'a Data) -> Self {
        match value {
            Data::Bool(value) => Self::Bool(*value),
            Data::Int(value) => Self::Int(*value),
            Data::Float(value) => Self::Float(f64::from(*value)),
            Data::String(value) => Self::Str(value),
            Data::List(_) | Data::Record(_) => unreachable!("checked binding types"),
        }
    }
}

/// The elements a run-time program may use, looked up by name.
///
/// [`Elements::new`] holds the built-in elements; add a library's with
/// [`with`](Self::with).
#[derive(Clone)]
pub struct Elements(pub(crate) Vec<&'static dyn Glue>);

impl Default for Elements {
    fn default() -> Self {
        Self::new()
    }
}

impl Elements {
    /// The built-in elements.
    pub fn new() -> Self {
        crate::elements::builtin()
    }
    /// Adds element `E`, replacing one of the same name.
    pub fn with<E: Element>(mut self) -> Self {
        let glue: &'static dyn Glue = const { &Erase::<E>(PhantomData) };
        self.0.retain(|other| other.spec().name != E::SPEC.name);
        self.0.push(glue);
        self
    }
    /// The specs of these elements, as a checker takes them.
    pub fn specs(&self) -> Vec<ElementSpec<'static>> {
        self.0.iter().map(|glue| *glue.spec()).collect()
    }
    pub(crate) fn get(&self, name: &str) -> &'static dyn Glue {
        *self.0.iter().find(|glue| glue.spec().name == name).unwrap()
    }
}

/// [`Element`] over an untyped [`Handle`].
pub(crate) trait Glue {
    fn spec(&self) -> &'static ElementSpec<'static>;
    fn create(&'static self, parent: &Container, args: &[(usize, Arg<'_>)]) -> Result<Handle>;
    fn set(&self, handle: &Handle, property: usize, value: Arg<'_>) -> Result;
    fn get(&self, handle: &Handle, field: usize) -> Result<Data>;
    fn listen(&self, handle: &Handle, event: usize, run: Box<dyn Fn() -> Result>) -> Result;
    fn parent(&self, handle: &Handle, child: usize) -> Container;
}

struct Erase<E>(PhantomData<fn() -> E>);

fn typed<E: Element>(handle: &Handle) -> &E::Handle {
    handle
        .typed
        .downcast_ref()
        .expect("a handle of its element")
}

impl<E: Element> Glue for Erase<E> {
    fn spec(&self) -> &'static ElementSpec<'static> {
        const { &E::SPEC }
    }
    fn create(&'static self, parent: &Container, args: &[(usize, Arg<'_>)]) -> Result<Handle> {
        let typed = E::create(parent, args)?;
        Ok(Handle {
            node: E::node(&typed).clone(),
            typed: Rc::new(typed) as Rc<dyn Any>,
            glue: Some(self),
        })
    }
    fn set(&self, handle: &Handle, property: usize, value: Arg<'_>) -> Result {
        E::set(typed::<E>(handle), property, value)
    }
    fn get(&self, handle: &Handle, field: usize) -> Result<Data> {
        E::get(typed::<E>(handle), field)
    }
    fn listen(&self, handle: &Handle, event: usize, run: Box<dyn Fn() -> Result>) -> Result {
        E::listen(typed::<E>(handle), event, run)
    }
    fn parent(&self, handle: &Handle, child: usize) -> Container {
        E::parent(typed::<E>(handle), child)
    }
}

/// Helpers for the code `element!` and `ui!` generate.
#[doc(hidden)]
pub mod __private {
    pub use aegle_ui::{Color, Container, Node, Result};

    pub use crate::handle::{apply, transitions};

    /// Calls a setter with its handle and value types known first.
    pub fn set<H, T>(handle: &H, value: T, set: impl FnOnce(&H, T) -> Result) -> Result {
        set(handle, value)
    }
    /// Calls a getter with its handle type known first.
    pub fn get<H, T>(handle: &H, get: impl FnOnce(&H) -> Result<T>) -> Result<T> {
        get(handle)
    }
    /// Calls an event registration with its handle type known first.
    pub fn listen<H>(
        handle: &H,
        run: Box<dyn Fn() -> Result>,
        listen: impl FnOnce(&H, Box<dyn Fn() -> Result>) -> Result,
    ) -> Result {
        listen(handle, run)
    }
    /// Calls a child placement with its handle type known first.
    pub fn place<H>(
        handle: &H,
        child: usize,
        place: impl FnOnce(&H, usize) -> Container,
    ) -> Container {
        place(handle, child)
    }
}
