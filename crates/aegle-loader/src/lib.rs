//! Runtime engine for dynamic Aegle markup.
//!
//! A [`Program`] is a checked multi-file markup program from `aegle-markup`.
//! Building it creates ordinary retained controls through `aegle-ui` and wires
//! typed states, property bindings, `on` event blocks, `if`/`for` blocks and
//! component instances. A state change re-evaluates only the bindings that read
//! it, then updates their controls; nothing is evaluated per frame. Bindings
//! live exactly as long as the controls they update.
//!
//! Elements are described once, with [`element!`]: the built-in ones in
//! [`elements`] and a control library's alike. A program checks against the
//! [`Elements`] it is given; `ui!` reads the same specs while compiling.
//!
//! `ui!` compiles static documents to direct construction. Documents with
//! dynamic features are checked at build time and compiled to code that
//! constructs the checked program for this engine, so those binaries carry the
//! engine but not the markup parser. [`Program::load`] parses documents at run
//! time, and [`View::reload`] replaces a built interface.
//!
//! `if` and `for` children live in an internal row or column that follows the
//! parent's direction and literal `gap`, and is hidden while empty.

mod actions;
mod build;
mod effects;
mod element;
pub mod elements;
mod eval;
mod handle;
mod layout;
mod motion;
mod reactive;
mod view;

use std::{fmt, fs::File, io::Read, path::Path, rc::Rc};

use aegle_markup::Span;
use aegle_ui::{Container, Result};

pub use actions::{Limits, action};
#[doc(hidden)]
pub use aegle_macros::__element;
pub use aegle_markup as markup;
#[doc(hidden)]
pub use element::__private;
pub use element::{Arg, Element, Elements};
pub use elements::{Column, Grid, RadioButton, Row, ScrollView, Stack, Tab, Text, TextArea};
pub use handle::Handle;
pub use view::{State, StateValue, View};

/// Declares markup elements: each one's spec, and the glue that creates its
/// control and applies its properties, events and `self` fields.
///
/// ```ignore
/// aegle::element! {
///     /// A selectable chip.
///     pub Chip {
///         style text interactive pressed;
///         create |parent, text: line = ""| Chip::new(parent, text);
///         set text: line => |chip, text| chip.set_text(text);
///         set selected: bool => |chip, on| chip.set_selected(on);
///         event changed => |chip, run| chip.on_change(move |_| run());
///         get selected: bool => |chip| chip.is_selected();
///     }
/// }
/// ```
///
/// `pub Name { ... }` implements `Element` for the handle type `Name`;
/// `pub Name(Handle) { ... }` defines a marker type `Name` for elements
/// sharing a handle type. Either way it defines a hidden macro `Name` that
/// `ui!` asks for the spec: re-export the handle or marker type at the
/// crate root, where the macro lives, so one `use` brings both.
///
/// Items, each ending with `;`:
/// - `layout leaf | box | flex | grid` (default `leaf`), and for containers
///   `children any | only Name | exactly N` (default `any`);
/// - `parent Name`: it may only be written directly inside `Name`;
/// - `style` followed by the groups `text interactive pressed indicator editor`;
/// - `create |parent, name: type = default, ...| expr` returning the handle;
///   a constructor argument without a default is required and literal;
/// - `set name: type => |handle, value| ...` for a settable, bindable property;
/// - `event name => |handle, run| ...` registering `run` as a handler;
/// - `get name: type => |handle| ...` for a `self.name` field;
/// - `children => |handle, index| container` where child `index` goes.
///
/// Types are `bool`, `int`, `int(min, max)`, `float`, `float(min)`,
/// `float(min, max)`, `fraction`, `length`, `string`, `line`, `color` and
/// `choice(a, b, ...)`, arriving as `bool`, `i64`, `f64`, `f32`, `f32`,
/// `&str`, `&str`, `Color` and `&str`.
#[macro_export]
macro_rules! element {
    ($($input:tt)*) => {
        $crate::__element! { $crate; $($input)* }
    };
}

/// A runtime value of a state, parameter or expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Data {
    /// `bool`.
    Bool(bool),
    /// `int`.
    Int(i64),
    /// `float`; always finite.
    Float(f32),
    /// `string`.
    String(Rc<str>),
    /// `list<...>` of ints, strings or records.
    List(Rc<[Data]>),
    /// A `record` value: its fields in declaration order.
    Record(Rc<[Data]>),
}

/// An evaluation failure: integer overflow or division by zero, a non-finite
/// float, an out-of-range conversion, a duplicate `for` key or a state value
/// of the wrong type. The span locates the expression in its markup file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    /// Human-readable reason.
    pub message: String,
    /// Responsible expression, or empty for host state assignments.
    pub span: Span,
    /// Path of the file holding `span`, when the engine knows it.
    pub file: Option<String>,
}

impl RuntimeError {
    pub(crate) fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span,
            file: None,
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (", self.message)?;
        if let Some(file) = &self.file {
            write!(f, "{file}, ")?;
        }
        write!(f, "markup bytes {}..{})", self.span.start, self.span.end)
    }
}

impl std::error::Error for RuntimeError {}

/// A checked markup program, shared by every view built from it.
///
/// Clones share one set of limits; host actions are registered per thread
/// with [`action`].
#[derive(Clone)]
pub struct Program(pub(crate) Rc<Shared>);

pub(crate) struct Shared {
    pub checked: aegle_markup::Program,
    /// The elements of `checked.elements`, in order.
    pub glue: Vec<&'static dyn element::Glue>,
    pub limits: std::cell::Cell<Limits>,
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Program")
            .field("templates", &self.0.checked.templates.len())
            .finish()
    }
}

impl Program {
    fn new(checked: aegle_markup::Program, elements: &Elements) -> Self {
        let glue = checked
            .elements
            .iter()
            .map(|name| elements.get(name))
            .collect();
        Self(Rc::new(Shared {
            checked,
            glue,
            limits: Default::default(),
        }))
    }

    /// Reads and checks a UTF-8 file and its imports against the built-in
    /// elements. Import paths resolve against the importing file's directory;
    /// each file may be at most 1 MiB.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        Self::load_with(path, &Elements::new())
    }

    /// Like [`load`](Self::load), against `elements`.
    pub fn load_with(path: impl AsRef<Path>, elements: &Elements) -> Result<Self> {
        let path = path.as_ref().to_str().ok_or("markup path is not UTF-8")?;
        Self::from_sources(path, elements, &mut |path| {
            let limit = aegle_markup::Limits::default().max_source_bytes as u64;
            let mut source = String::new();
            File::open(path)
                .and_then(|file| file.take(limit + 1).read_to_string(&mut source))
                .map_err(|error| error.to_string())?;
            Ok(source)
        })
    }

    /// Checks `entry` and its imports against `elements`, read through
    /// `read` by normalized path. See [`aegle_markup::compile`] for path
    /// rules and diagnostics.
    pub fn from_sources(
        entry: &str,
        elements: &Elements,
        read: &mut dyn FnMut(&str) -> std::result::Result<String, String>,
    ) -> Result<Self> {
        let (program, _) = aegle_markup::compile(entry, &elements.specs(), read)?;
        Ok(Self::new(program, elements))
    }

    /// Wraps a program produced by [`markup::check_program`]; only code that
    /// `ui!` generates calls it, with a program it checked while compiling.
    /// It is not an entry point: the engine relies on the checker's
    /// invariants and panics on a program that skipped it.
    #[doc(hidden)]
    pub fn from_checked(program: markup::Program, elements: &Elements) -> Self {
        Self::new(program, elements)
    }

    /// Replaces the execution limits for views built afterwards.
    pub fn set_limits(&self, limits: Limits) {
        self.0.limits.set(limits);
    }

    /// Builds a document whose root is not a Window as the last child of `parent`.
    /// A failure removes everything this call created.
    pub fn build(&self, parent: &Container) -> Result<View> {
        view::fragment(self, parent, &|_, _| None)
    }

    /// Opens a document whose root is a Window. A failure closes that window.
    #[cfg(any(
        all(feature = "wayland", target_os = "linux"),
        all(feature = "windows", target_os = "windows")
    ))]
    pub fn open(&self, app: &aegle_app::App) -> Result<View> {
        view::window(self, app)
    }
}
