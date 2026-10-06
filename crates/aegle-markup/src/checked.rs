//! Checked dynamic programs, ready for a runtime engine.

use std::rc::Rc;

use crate::{Expr, Kind, PropertyName, Record, Span, Type, Value};

/// A checked program: the entry root and every declared component.
#[derive(Debug)]
pub struct Program {
    /// `templates[0]` is the entry root; components follow in file order.
    pub templates: Vec<Template>,
    /// Entry IDs with their component kinds, in handle order.
    pub ids: Vec<(String, Kind)>,
    /// Declared records, in file order; `ExprKind::Record` indexes this.
    pub records: Vec<Record>,
    /// Every `host.name(...)` call, for validation against the host's actions.
    pub host_calls: Vec<HostCall>,
    /// Paths of the files, entry first, when the program came from [`crate::compile`].
    pub files: Vec<String>,
}

/// A call of a host action with the types of its arguments.
#[derive(Debug)]
pub struct HostCall {
    /// Action name.
    pub name: String,
    /// Argument types, in order.
    pub types: Vec<Type>,
    /// Index of the file holding the call.
    pub file: usize,
    /// Source range of the statement.
    pub span: Span,
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
    /// Events the body may emit: name and optional value type.
    pub events: Vec<(String, Option<Type>)>,
    /// Whether the body places the instance's children with `slot`.
    pub slot: bool,
    /// Index of the file declaring this template.
    pub file: usize,
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
    /// Event handlers of a built-in control.
    pub events: Vec<(EventKind, Rc<[Step]>)>,
    /// Handlers of a component instance's declared events.
    pub handlers: Vec<Handler>,
    /// Children in source order.
    pub children: Vec<Child>,
    /// Children of a component instance, placed by the component's `slot`.
    pub slot: Option<Rc<[Child]>>,
    /// Source range of the declaration.
    pub span: Span,
}

/// A handler of a component event, written `on name(value) { ... }`.
#[derive(Debug)]
pub struct Handler {
    /// Index into the component's declared events.
    pub event: usize,
    /// Whether the handler binds the event's value as its first local.
    pub binds_value: bool,
    /// Statements run when the instance emits the event.
    pub steps: Rc<[Step]>,
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
    /// Children built once per item of a list, keyed by the key expression
    /// (read with the item as the innermost `for` item) or by the scalar item.
    For(Rc<Expr>, Option<Rc<Expr>>, Rc<[Child]>),
    /// The children of the component instance being built.
    Slot,
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
    /// Evaluates once and appends a local.
    Let(Expr),
    /// Calls the host action with the arguments.
    Host(String, Vec<Expr>, Span),
    /// Raises the component's declared event with this index, carrying a value.
    Emit(usize, Option<Expr>),
}
