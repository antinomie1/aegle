//! Bounded parsing of UTF-8 `.aegle` interface markup.
//!
//! Structural nodes and literal properties are supported. Declarations are
//! separated by a newline or semicolon; the last declaration may end at `}`.
//! Strings use JSON escapes, comments start with `//`, and lengths use `dp`.
//! Expressions and executable statements are explicitly rejected.
//!
//! Parsing does not create controls. A host validates the resulting document
//! against its component schema before constructing a retained interface.

mod ast;
mod error;
mod lexer;
mod parse;
mod schema;

pub use ast::{Document, Limits, Node, Property, Span, Value};
pub use error::Error;
pub use parse::{parse, parse_with_limits};
pub use schema::{CheckedDocument, CheckedNode, CheckedProperty, Kind, PropertyName, check};
