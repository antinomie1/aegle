//! Checked dynamic programs, ready for a runtime engine.

use std::rc::Rc;

use crate::{Expr, PropertyName, Record, Span, Type, Value};

/// A checked program: the entry root and every declared component.
#[derive(Debug)]
pub struct Program {
    /// `templates[0]` is the entry root; components follow in file order.
    pub templates: Vec<Template>,
    /// Names of the elements the program creates; [`ElementKind::Control`]
    /// indexes this.
    pub elements: Vec<String>,
    /// Entry IDs with what they name, in handle order.
    pub ids: Vec<(String, ElementKind)>,
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

/// The document window, one element or a component instance.
#[derive(Debug)]
pub struct Element {
    /// What it creates.
    pub kind: ElementKind,
    /// Index into [`Program::ids`] for an entry-level named control.
    pub id: Option<usize>,
    /// Node and element properties: literals are validated, expressions are typed.
    pub properties: Vec<(Prop, Bound)>,
    /// Component arguments by parameter; `None` uses the default.
    pub arguments: Vec<Option<Rc<Expr>>>,
    /// Event handlers of an element, by index into its spec's events.
    pub events: Vec<(usize, Rc<[Step]>)>,
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
    /// The native window of a document root.
    Window,
    /// A control of the element named `Program::elements[index]`.
    Control(usize),
    /// An instance of `Program::templates[index]`.
    Component(usize),
}

/// A property of a window or element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prop {
    /// A property every node has.
    Node(PropertyName),
    /// The element property with this index in its spec.
    Element(usize),
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
