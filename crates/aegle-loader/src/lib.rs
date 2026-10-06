//! Runtime engine for dynamic Aegle markup.
//!
//! A [`Program`] is a checked multi-file markup program from `aegle-markup`.
//! Building it creates ordinary retained controls through `aegle-app` and wires
//! typed states, property bindings, `on` event blocks, `if`/`for` blocks and
//! component instances. A state change re-evaluates only the bindings that read
//! it, then updates their controls; nothing is evaluated per frame. Bindings
//! live exactly as long as the controls they update.
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
mod eval;
mod handle;
mod reactive;
mod view;

use std::{fmt, fs::File, io::Read, path::Path, rc::Rc};

use aegle_app::{Container, Result};
use aegle_markup::Span;

pub use actions::{Limits, register_shared as action};
pub use aegle_markup as markup;
pub use handle::{FromHandle, Handle};
pub use view::{State, StateValue, View};

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
/// Clones share one registry of host actions and one set of limits.
#[derive(Clone)]
pub struct Program(pub(crate) Rc<Shared>);

pub(crate) struct Shared {
    pub checked: aegle_markup::Program,
    pub actions: actions::Actions,
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
    fn new(checked: aegle_markup::Program) -> Self {
        Self(Rc::new(Shared {
            checked,
            actions: Default::default(),
            limits: Default::default(),
        }))
    }

    /// Reads and checks a UTF-8 file and its imports. Import paths resolve
    /// against the importing file's directory; each file may be at most 1 MiB.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_str().ok_or("markup path is not UTF-8")?;
        Self::from_sources(path, &mut |path| {
            let limit = aegle_markup::Limits::default().max_source_bytes as u64;
            let mut source = String::new();
            File::open(path)
                .and_then(|file| file.take(limit + 1).read_to_string(&mut source))
                .map_err(|error| error.to_string())?;
            Ok(source)
        })
    }

    /// Checks `entry` and its imports, read through `read` by normalized path.
    /// See [`aegle_markup::compile`] for path rules and diagnostics.
    pub fn from_sources(
        entry: &str,
        read: &mut dyn FnMut(&str) -> std::result::Result<String, String>,
    ) -> Result<Self> {
        let (program, _) = aegle_markup::compile(entry, read)?;
        Ok(Self::new(program))
    }

    /// Wraps a program already checked by [`markup::check_program`], as
    /// compiled views construct it without parsing.
    pub fn from_checked(program: markup::Program) -> Self {
        Self::new(program)
    }

    /// Registers a host action that `host.name(...)` statements call, with the
    /// markup types of its arguments. Building a view fails, before anything is
    /// mounted, if a call has no action or different argument types. The action
    /// runs outside every UI borrow, like an event handler; an error it returns
    /// stops the handler. Registering a name again replaces the action.
    pub fn action(
        &self,
        name: &str,
        params: &[markup::Type],
        run: impl FnMut(&[Data]) -> Result + 'static,
    ) {
        self.0.actions.register(name, params, run);
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
