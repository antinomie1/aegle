//! Source positions and the owned structural document.

/// A half-open byte range in the original UTF-8 source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// First byte, inclusive.
    pub start: usize,
    /// Last byte, exclusive.
    pub end: usize,
}

/// A parsed interface with exactly one root component.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    /// The root component and its nested declarations.
    pub root: Node,
}

/// A component declaration. Names are case-sensitive ASCII identifiers.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Component type name.
    pub name: String,
    /// Property assignments, in source order.
    pub properties: Vec<Property>,
    /// Nested components, in source order.
    pub children: Vec<Node>,
    /// Entire declaration, including its closing brace.
    pub span: Span,
}

/// One literal property assignment.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    /// Property name.
    pub name: String,
    /// Parsed literal or enum/name identifier.
    pub value: Value,
    /// Entire assignment, excluding its separator.
    pub span: Span,
    /// Source range of just the value.
    pub value_span: Span,
}

/// Literal values supported by the structural markup parser.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Decoded UTF-8 string, with JSON escape semantics.
    String(String),
    /// `true` or `false`.
    Bool(bool),
    /// A finite 32-bit number without a unit.
    Number(f32),
    /// A finite logical length, written with the `dp` suffix.
    Length(f32),
    /// An unquoted identifier, such as a node ID or enum value.
    Identifier(String),
}

/// Explicit parsing budgets; the root counts as one node and one level.
///
/// Zero is a valid restrictive budget. Source size is checked before lexing;
/// node and depth budgets are checked before descending into each node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum source size in UTF-8 bytes.
    pub max_source_bytes: usize,
    /// Maximum nesting depth, including the root. Must not exceed 256.
    pub max_depth: usize,
    /// Maximum total number of component nodes.
    pub max_nodes: usize,
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
