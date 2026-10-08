//! Source positions and the owned parsed document.

/// A half-open byte range in the original UTF-8 source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// First byte, inclusive.
    pub start: usize,
    /// Last byte, exclusive.
    pub end: usize,
}

/// A parsed file: imports, component declarations and an optional root.
///
/// Entry documents require a root; imported files declare only components.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    /// `use "relative.aegle"` imports, in source order.
    pub uses: Vec<Use>,
    /// `component Name(...) { ... }` declarations, in source order.
    pub components: Vec<Component>,
    /// `record Name { field: type }` declarations, in source order.
    pub records: Vec<Record>,
    /// The root component instance, if any.
    pub root: Option<Node>,
}

impl Document {
    /// Whether the document uses only literal structure, so it can be compiled
    /// to direct construction without states, events, blocks or components.
    pub fn is_static(&self) -> bool {
        self.uses.is_empty()
            && self.components.is_empty()
            && self.records.is_empty()
            && self.root.as_ref().is_none_or(Node::is_static)
    }
}

/// An import of every component declared in another file.
#[derive(Clone, Debug, PartialEq)]
pub struct Use {
    /// Path relative to the importing file, as written.
    pub path: String,
    /// The whole declaration.
    pub span: Span,
}

/// A named group of typed fields, such as the items of a `list<Task>`.
///
/// Fields are `bool`, `int`, `float` or `string`. A value is built with a
/// positional call, `Task(1, "write")`, and read with `task.title`.
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    /// Case-sensitive name, global to the program like component names.
    pub name: String,
    /// Fields in declaration order.
    pub fields: Vec<(String, Type)>,
    /// The whole declaration.
    pub span: Span,
}

/// An event a component can raise with `emit`, optionally carrying one value.
#[derive(Clone, Debug, PartialEq)]
pub struct EventDecl {
    /// Event name.
    pub name: String,
    /// Type of the carried value.
    pub ty: Option<Type>,
    /// The whole declaration.
    pub span: Span,
}

/// A reusable component with typed input parameters and one root node.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    /// Case-sensitive name used to instantiate the component.
    pub name: String,
    /// Typed parameters in declaration order.
    pub params: Vec<Param>,
    /// Events the body may `emit`, handled by `on name { ... }` on instances.
    pub events: Vec<EventDecl>,
    /// The single root node of the component body.
    pub root: Node,
    /// The whole declaration.
    pub span: Span,
}

/// A typed component parameter with an optional constant default.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Declared type.
    pub ty: Type,
    /// Default value; a parameter without one is required.
    pub default: Option<Expr>,
    /// The whole parameter.
    pub span: Span,
}

/// Value types of states, parameters and expressions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// `bool`.
    Bool,
    /// `int`, a signed 64-bit integer.
    Int,
    /// `float`, a finite 32-bit number.
    Float,
    /// `string`, UTF-8 text.
    String,
    /// `list<T>` of int, string or record items; scalar items double as `for` keys.
    List(Box<Type>),
    /// A declared `record`, by name.
    Record(String),
}

/// A component instance with properties, local declarations and children.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Component type name.
    pub name: String,
    /// Property assignments, in source order.
    pub properties: Vec<Property>,
    /// `state name: type = value` declarations.
    pub states: Vec<State>,
    /// `on event { ... }` handlers.
    pub events: Vec<Event>,
    /// Nested nodes and structural blocks, in source order.
    pub children: Vec<Item>,
    /// Entire declaration, including its closing brace.
    pub span: Span,
}

impl Node {
    fn is_static(&self) -> bool {
        self.states.is_empty()
            && self.events.is_empty()
            && self
                .properties
                .iter()
                .all(|property| match &property.value {
                    Value::Expr(expr) => crate::schema::constant(expr).is_some(),
                    _ => true,
                })
            && self.children.iter().all(|item| match item {
                Item::Node(node) => node.is_static(),
                Item::If(..) | Item::For(..) | Item::Slot => false,
            })
    }
}

/// A child of a node.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A nested component instance.
    Node(Node),
    /// `if condition { ... } else { ... }`.
    If(Expr, Vec<Item>, Vec<Item>),
    /// `for name in list key expression { ... }`; scalar items are their own key.
    For(String, Expr, Option<Expr>, Vec<Item>),
    /// `slot`: where a component places the children of its instance.
    Slot,
}

/// A typed local state declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    /// State name.
    pub name: String,
    /// Declared type.
    pub ty: Type,
    /// Initial value.
    pub value: Expr,
    /// The whole declaration.
    pub span: Span,
}

/// An event handler block.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// Event name, such as `clicked`.
    pub name: String,
    /// Name binding the carried value, in `on changed(value) { ... }`.
    pub param: Option<String>,
    /// Statements run when the event fires.
    pub body: Vec<Statement>,
    /// The whole handler.
    pub span: Span,
}

/// An executable statement inside an event block.
#[derive(Clone, Debug, PartialEq)]
pub enum Statement {
    /// `name = value`, `name += value` or `name -= value`.
    Assign {
        /// Assigned state name.
        target: String,
        /// `=`, `+=` or `-=`.
        operator: &'static str,
        /// Assigned or combined value.
        value: Expr,
        /// The whole statement.
        span: Span,
    },
    /// `if condition { ... } else { ... }`.
    If(Expr, Vec<Statement>, Vec<Statement>),
    /// `let name = value`, visible to the statements after it in this block.
    Let {
        /// Local name.
        name: String,
        /// Value, evaluated once.
        value: Expr,
        /// The whole statement.
        span: Span,
    },
    /// `host.name(arguments)`, calling an action the host registered.
    Host {
        /// Action name.
        name: String,
        /// Arguments in order.
        arguments: Vec<Expr>,
        /// The whole statement.
        span: Span,
    },
    /// `emit name(value)`, raising a declared component event.
    Emit {
        /// Event name.
        name: String,
        /// Carried value, present exactly when the event declares a type.
        value: Option<Expr>,
        /// The whole statement.
        span: Span,
    },
}

/// One property assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    /// Property name.
    pub name: String,
    /// Parsed literal, identifier or expression.
    pub value: Value,
    /// Entire assignment, excluding its separator.
    pub span: Span,
    /// Source range of just the value.
    pub value_span: Span,
}

/// Literal values, plus expressions bound to states.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Decoded UTF-8 string, with JSON escape semantics.
    String(String),
    /// `true` or `false`.
    Bool(bool),
    /// A digit-only integer without a unit that fits i64.
    Int(i64),
    /// A finite 32-bit number without a unit.
    Number(f32),
    /// A finite logical length, written with the `dp` suffix.
    Length(f32),
    /// A finite percentage of the parent's size, written with the `%` suffix.
    Percent(f32),
    /// A finite grid track share, written with the `fr` suffix.
    Fraction(f32),
    /// A constant list of literals and bare identifiers, such as
    /// `[8dp, auto]`; only layout properties, `shadow` and gradient stops
    /// accept one.
    List(Vec<Value>),
    /// A constant function: `repeat`, `minmax`, `fit_content`, `token`,
    /// `linear` or `radial` with constant arguments, or `calc` folded to
    /// `[percent, length]`, such as `calc(100% - 8dp)` as `calc(100%, -8dp)`.
    Call(String, Vec<Value>),
    /// Exact nonnegative whole milliseconds, written as decimal digits and `ms`.
    Duration(u64),
    /// Unpremultiplied sRGB bytes, written as `#RRGGBB` or `#RRGGBBAA`.
    Color([u8; 4]),
    /// An unquoted identifier, such as an enum value or a state name.
    Identifier(String),
    /// Any other expression; only dynamic documents accept it.
    Expr(Box<Expr>),
}

/// An expression with its source range.
#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    /// Operation.
    pub kind: ExprKind,
    /// Source range.
    pub span: Span,
}

/// Expression operations. Checking replaces names with resolved references.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    /// A literal; never `Value::Expr`.
    Literal(Value),
    /// An unresolved state, parameter or loop item name.
    Name(String),
    /// `self.field` inside an event block.
    SelfField(String),
    /// `!value` or `-value`.
    Unary(&'static str, Box<Expr>),
    /// Arithmetic, comparison or logical operator.
    Binary(&'static str, Box<Expr>, Box<Expr>),
    /// A built-in function (`str`, `len`, `int`, `float`) or a record constructor.
    Call(String, Vec<Expr>),
    /// `[a, b, ...]`.
    List(Vec<Expr>),
    /// `value.field` before checking.
    Field(Box<Expr>, String),
    /// A checked field read: field number of a record value.
    FieldAt(Box<Expr>, usize),
    /// A checked record construction: record number and one value per field.
    Record(usize, Vec<Expr>),
    /// A checked reference.
    Ref(Ref),
}

/// A resolved name in a checked expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ref {
    /// Local state of the current document or component instance.
    State(usize),
    /// Parameter of the current component instance.
    Param(usize),
    /// Item of an enclosing `for`, outermost first.
    Item(usize),
    /// A `let` local or the value carried by an event, in declaration order.
    Local(usize),
}

/// Explicit parsing budgets; the root counts as one node and one level.
///
/// Zero is a valid restrictive budget. Source size is checked before lexing;
/// node and depth budgets are checked before descending into each node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum source size in UTF-8 bytes.
    pub max_source_bytes: usize,
    /// Maximum nesting depth, including the root. Must not exceed
    /// [`Self::MAX_DEPTH`].
    pub max_depth: usize,
    /// Maximum total number of component nodes.
    pub max_nodes: usize,
}

impl Limits {
    /// The deepest nesting any parse may allow. The checkers reject deeper
    /// hand-built documents, so building never recurses further.
    pub const MAX_DEPTH: usize = 256;
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_bytes: 1024 * 1024,
            max_depth: 64,
            max_nodes: 10_000,
        }
    }
}
