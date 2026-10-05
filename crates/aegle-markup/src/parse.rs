use crate::lexer::{Kind, Lexer, Token};
use crate::{Document, Error, Limits, Node, Property, Span, Value};

/// Parses one structural interface using the default resource limits.
pub fn parse(source: &str) -> Result<Document, Error> {
    parse_with_limits(source, &Limits::default())
}

/// Parses one structural interface within explicit source, depth and node limits.
///
/// All source must belong to the single root, separators or comments. Unknown
/// types and property names are left to the host's schema validation.
pub fn parse_with_limits(source: &str, limits: &Limits) -> Result<Document, Error> {
    if limits.max_depth > 256 {
        return Err(Error::new(
            Span { start: 0, end: 0 },
            "configured depth limit must not exceed 256",
        ));
    }
    if source.len() > limits.max_source_bytes {
        return Err(Error::new(
            Span { start: 0, end: 0 },
            "markup source exceeds its byte limit",
        ));
    }
    let mut lexer = Lexer::new(source);
    let current = lexer.next()?;
    let mut parser = Parser {
        lexer,
        current,
        limits,
        nodes: 0,
    };
    parser.separators()?;
    let (name, start) = parser.identifier()?;
    let root = parser.node(name, start, 1)?;
    parser.separators()?;
    if !matches!(parser.current.kind, Kind::End) {
        return Err(parser.error("expected end of source after the root component"));
    }
    Ok(Document { root })
}

struct Parser<'a, 'l> {
    lexer: Lexer<'a>,
    current: Token<'a>,
    limits: &'l Limits,
    nodes: usize,
}

impl Parser<'_, '_> {
    fn advance(&mut self) -> Result<Token<'_>, Error> {
        let next = self.lexer.next()?;
        Ok(std::mem::replace(&mut self.current, next))
    }

    fn error(&self, message: &str) -> Error {
        Error::new(self.current.span, message)
    }

    fn separators(&mut self) -> Result<(), Error> {
        while matches!(self.current.kind, Kind::Separator) {
            self.advance()?;
        }
        Ok(())
    }

    fn identifier(&mut self) -> Result<(String, usize), Error> {
        match self.current.kind {
            Kind::Identifier(name) => {
                if matches!(name, "state" | "on" | "if" | "for" | "component" | "use") {
                    return Err(self.error(
                        "executable statements and component declarations are not supported yet",
                    ));
                }
                let result = (name.to_owned(), self.current.span.start);
                self.advance()?;
                Ok(result)
            }
            _ => Err(self.error("expected an ASCII component or property name")),
        }
    }

    fn node(&mut self, name: String, start: usize, depth: usize) -> Result<Node, Error> {
        if depth > self.limits.max_depth {
            return Err(self.error("component nesting exceeds its depth limit"));
        }
        if self.nodes >= self.limits.max_nodes {
            return Err(self.error("component count exceeds its node limit"));
        }
        self.nodes += 1;
        if !matches!(self.current.kind, Kind::Open) {
            return Err(self.error("expected '{' after the component name"));
        }
        self.advance()?;
        let mut node = Node {
            name,
            properties: Vec::new(),
            children: Vec::new(),
            span: Span { start, end: start },
        };
        self.separators()?;
        while !matches!(self.current.kind, Kind::Close) {
            if matches!(self.current.kind, Kind::End) {
                return Err(self.error("expected '}' to close the component"));
            }
            let (name, start) = self.identifier()?;
            match self.current.kind {
                Kind::Open => node.children.push(self.node(name, start, depth + 1)?),
                Kind::Colon => {
                    self.advance()?;
                    let value_span = self.current.span;
                    let value = self.value()?;
                    node.properties.push(Property {
                        name,
                        value,
                        span: Span {
                            start,
                            end: value_span.end,
                        },
                        value_span,
                    });
                }
                _ => return Err(self.error("expected ':' for a property or '{' for a child")),
            }
            match self.current.kind {
                Kind::Close => {}
                Kind::Separator => self.separators()?,
                _ => {
                    return Err(self.error(
                        "expected a newline, ';' or '}' after the declaration; expressions are not supported",
                    ));
                }
            }
        }
        node.span.end = self.current.span.end;
        self.advance()?;
        Ok(node)
    }

    fn value(&mut self) -> Result<Value, Error> {
        let token = self.advance()?;
        match token.kind {
            Kind::String(value) => Ok(Value::String(value)),
            Kind::Number(value) => Ok(Value::Number(value)),
            Kind::Length(value) => Ok(Value::Length(value)),
            Kind::Duration(value) => Ok(Value::Duration(value)),
            Kind::Color(value) => Ok(Value::Color(value)),
            Kind::Identifier("true") => Ok(Value::Bool(true)),
            Kind::Identifier("false") => Ok(Value::Bool(false)),
            Kind::Identifier("NaN" | "Infinity" | "inf") => {
                Err(Error::new(token.span, "number must be finite"))
            }
            Kind::Identifier(value) => Ok(Value::Identifier(value.to_owned())),
            _ => Err(Error::new(token.span, "expected a literal property value")),
        }
    }
}
