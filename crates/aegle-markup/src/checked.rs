//! Checked dynamic programs, ready for a runtime engine.

use std::rc::Rc;

use crate::{Expr, Kind, PropertyName, Span, Type, Value};

/// A checked program: the entry root and every declared component.
#[derive(Debug)]
pub struct Program {
    /// `templates[0]` is the entry root; components follow in file order.
    pub templates: Vec<Template>,
    /// Entry IDs with their component kinds, in handle order.
    pub ids: Vec<(String, Kind)>,
}

/// The entry document root or one component body.
#[derive(Debug)]
pub struct Template {
    /// Component name; the entry template uses its root component name.
    pub name: String,
    /// Parameters with optional literal defaults.
    pub params: Vec<(String, Type, Option<Expr>)>,
    /// States in declaration order. Initial values may read parameters and
    /// earlier states; they are evaluated once per instance.
    pub states: Vec<(String, Type, Expr)>,
    /// The single root element.
    pub root: Element,
}

/// One built-in control or component instance.
#[derive(Debug)]
pub struct Element {
    /// Built-in kind or component template.
    pub kind: ElementKind,
    /// Index into [`Program::ids`] for an entry-level named control.
    pub id: Option<usize>,
    /// Built-in properties: literals are validated, expressions are typed.
    pub properties: Vec<(PropertyName, Bound)>,
    /// Component arguments by parameter; `None` uses the default.
    pub arguments: Vec<Option<Rc<Expr>>>,
    /// Event handlers.
    pub events: Vec<(EventKind, Rc<[Step]>)>,
    /// Children in source order.
    pub children: Vec<Child>,
    /// Source range of the declaration.
    pub span: Span,
}

/// What an element creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementKind {
    /// A built-in control.
    Builtin(Kind),
    /// An instance of `Program::templates[index]`.
    Component(usize),
}

/// A property value.
#[derive(Debug)]
pub enum Bound {
    /// A validated literal, applied once.
    Literal(Value),
    /// A typed expression re-applied when the states it reads change.
    Expr(Rc<Expr>),
}

/// A child of a container element.
#[derive(Debug)]
pub enum Child {
    /// A control or component instance.
    Element(Element),
    /// Children built only while the boolean condition holds, else the alternative.
    If(Rc<Expr>, Rc<[Child]>, Rc<[Child]>),
    /// Children built once per item of an `int` or `string` list, keyed by item.
    For(Rc<Expr>, Rc<[Child]>),
}

/// Events of built-in controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// Button activation: `on clicked`.
    Clicked,
    /// A user change of a check box, switch or slider: `on changed`.
    Changed,
    /// Enter in a single-line text field: `on submitted`.
    Submitted,
}

/// A checked event statement.
#[derive(Debug)]
pub enum Step {
    /// Assigns, adds to or subtracts from a state.
    Assign(usize, &'static str, Expr),
    /// Runs one branch.
    If(Expr, Vec<Step>, Vec<Step>),
}
