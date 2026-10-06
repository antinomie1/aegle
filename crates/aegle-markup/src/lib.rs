//! Bounded parsing of UTF-8 `.aegle` interface markup.
//!
//! Declarations are separated by a newline or semicolon; the last declaration
//! may end at `}`. Strings use JSON escapes, comments start with `//`, and
//! lengths use `dp`. Colors use six or eight hexadecimal digits: `#RRGGBB` or
//! `#RRGGBBAA`. Durations use exact whole milliseconds, such as `120ms`.
//!
//! Static documents hold only nodes and literal properties; [`check`] validates
//! them for direct construction. Dynamic documents add typed `state`, property
//! expressions, `on` event blocks, `if`/`for` blocks, `component` declarations
//! and `use` imports; [`compile`] loads imports and [`check_program`] resolves
//! and types them for a runtime engine.
//!
//! Parsing and checking never create controls or perform I/O themselves.

mod ast;
mod checked;
mod error;
mod expr;
mod files;
mod lexer;
mod parse;
mod program;
mod schema;
mod typing;

pub use ast::{
    Component, Document, Event, EventDecl, Expr, ExprKind, Item, Limits, Node, Param, Property,
    Record, Ref, Span, State, Statement, Type, Use, Value,
};
pub use checked::{
    Bound, Child, Element, ElementKind, EventKind, Handler, HostCall, Program, Step, Template,
};
pub use error::Error;
pub use files::{File, ProgramError, compile};
pub use parse::{parse, parse_with_limits};
pub use program::check_program;
pub use schema::{CheckedDocument, CheckedNode, CheckedProperty, Kind, PropertyName, check};
