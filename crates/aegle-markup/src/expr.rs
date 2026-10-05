//! Types and expressions, parsed by precedence climbing.

use crate::lexer::Kind;
use crate::parse::Parser;
use crate::{Error, Expr, ExprKind, Type, Value};

/// Binary operators from lowest to highest precedence; each level is left-associative.
const LEVELS: [&[&str]; 5] = [
    &["||"],
    &["&&"],
    &["==", "!=", "<", "<=", ">", ">="],
    &["+", "-"],
    &["*", "/", "%"],
];

impl From<Expr> for Value {
    /// A literal or bare name stays a plain property value; anything else is an expression.
    fn from(expr: Expr) -> Self {
        match expr.kind {
            ExprKind::Literal(value) => value,
            ExprKind::Name(name) => Value::Identifier(name),
            _ => Value::Expr(Box::new(expr)),
        }
    }
}

impl Parser<'_, '_> {
    pub fn ty(&mut self) -> Result<Type, Error> {
        let (name, _) = self.identifier()?;
        Ok(match name.as_str() {
            "bool" => Type::Bool,
            "int" => Type::Int,
            "float" => Type::Float,
            "string" => Type::String,
            "list" => {
                self.expect("<", "expected '<' after list")?;
                let item = self.ty()?;
                if !matches!(item, Type::Int | Type::String) {
                    return Err(self.error("list items must be int or string"));
                }
                self.expect(">", "expected '>' to close the list type")?;
                Type::List(Box::new(item))
            }
            _ => return Err(self.error("expected bool, int, float, string or list<...>")),
        })
    }

    pub fn expr(&mut self) -> Result<Expr, Error> {
        self.binary(0, 0)
    }

    fn binary(&mut self, level: usize, depth: usize) -> Result<Expr, Error> {
        if level == LEVELS.len() {
            return self.unary(depth);
        }
        let mut left = self.binary(level + 1, depth)?;
        while let Kind::Punct(operator) = self.current.kind {
            if !LEVELS[level].contains(&operator) {
                break;
            }
            self.advance()?;
            let right = self.binary(level + 1, depth)?;
            let span = crate::Span {
                start: left.span.start,
                end: right.span.end,
            };
            left = Expr {
                kind: ExprKind::Binary(operator, Box::new(left), Box::new(right)),
                span,
            };
        }
        Ok(left)
    }

    fn unary(&mut self, depth: usize) -> Result<Expr, Error> {
        if depth > self.limits.max_depth {
            return Err(self.error("expression nesting exceeds its depth limit"));
        }
        let start = self.current.span.start;
        let Kind::Punct(operator @ ("!" | "-")) = self.current.kind else {
            return self.primary(depth);
        };
        self.advance()?;
        let operand = self.unary(depth + 1)?;
        let span = self.span(start);
        // Negative numeric literals stay literals for plain property values.
        let kind = match (operator, operand.kind) {
            ("-", ExprKind::Literal(Value::Int(n))) => ExprKind::Literal(Value::Int(-n)),
            ("-", ExprKind::Literal(Value::Number(n))) => ExprKind::Literal(Value::Number(-n)),
            ("-", ExprKind::Literal(Value::Length(n))) => ExprKind::Literal(Value::Length(-n)),
            ("-", ExprKind::Literal(Value::Duration(_))) => {
                return Err(Error::new(
                    span,
                    "duration requires nonnegative whole milliseconds",
                ));
            }
            (operator, kind) => ExprKind::Unary(
                operator,
                Box::new(Expr {
                    kind,
                    span: operand.span,
                }),
            ),
        };
        Ok(Expr { kind, span })
    }

    fn primary(&mut self, depth: usize) -> Result<Expr, Error> {
        let start = self.current.span.start;
        let token = self.advance()?;
        let kind = match token.kind {
            Kind::String(value) => ExprKind::Literal(Value::String(value)),
            Kind::Integer(value) => ExprKind::Literal(Value::Int(value)),
            Kind::Number(value) => ExprKind::Literal(Value::Number(value)),
            Kind::Length(value) => ExprKind::Literal(Value::Length(value)),
            Kind::Duration(value) => ExprKind::Literal(Value::Duration(value)),
            Kind::Color(value) => ExprKind::Literal(Value::Color(value)),
            Kind::Identifier("true") => ExprKind::Literal(Value::Bool(true)),
            Kind::Identifier("false") => ExprKind::Literal(Value::Bool(false)),
            Kind::Identifier("NaN" | "Infinity" | "inf") => {
                return Err(Error::new(token.span, "number must be finite"));
            }
            Kind::Identifier("self") => {
                self.expect(".", "expected '.' and a field after self")?;
                ExprKind::SelfField(self.identifier()?.0)
            }
            Kind::Identifier(name) if matches!(self.current.kind, Kind::Punct("(")) => {
                self.advance()?;
                let arguments = self.list(")", depth)?;
                ExprKind::Call(name.to_owned(), arguments)
            }
            Kind::Identifier(name) => ExprKind::Name(name.to_owned()),
            Kind::Punct("(") => {
                let inner = self.binary(0, depth + 1)?;
                self.expect(")", "expected ')'")?;
                return Ok(Expr {
                    kind: inner.kind,
                    span: self.span(start),
                });
            }
            Kind::Punct("[") => ExprKind::List(self.list("]", depth)?),
            _ => return Err(Error::new(token.span, "expected a value or expression")),
        };
        Ok(Expr {
            kind,
            span: self.span(start),
        })
    }

    /// Parses comma-separated expressions up to and including `close`.
    fn list(&mut self, close: &str, depth: usize) -> Result<Vec<Expr>, Error> {
        let mut items = Vec::new();
        while !matches!(self.current.kind, Kind::Punct(p) if p == close) {
            items.push(self.binary(0, depth + 1)?);
            if !matches!(self.current.kind, Kind::Punct(p) if p == close) {
                self.expect(",", "expected ',' between values")?;
            }
        }
        self.advance()?;
        Ok(items)
    }
}
