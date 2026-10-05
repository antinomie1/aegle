use crate::lexer::{Kind, Lexer, Token};
use crate::{
    Component, Document, Error, Event, Item, Limits, Node, Param, Property, Span, State, Statement,
    Value,
};

/// Parses one file using the default resource limits.
pub fn parse(source: &str) -> Result<Document, Error> {
    parse_with_limits(source, &Limits::default())
}

/// Parses one file within explicit source, depth and node limits.
///
/// A file holds `use` imports and `component` declarations followed by at most
/// one root node. Unknown types and property names are left to checking.
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
        last_end: 0,
        limits,
        nodes: 0,
    };
    let mut document = Document {
        uses: Vec::new(),
        components: Vec::new(),
        root: None,
    };
    parser.separators()?;
    while !matches!(parser.current.kind, Kind::End) {
        if document.root.is_some() {
            return Err(parser.error("expected end of source after the root component"));
        }
        let start = parser.current.span.start;
        match parser.current.kind {
            Kind::Identifier("use") => {
                parser.advance()?;
                let Kind::String(path) = parser.advance()?.kind else {
                    return Err(Error::new(parser.span(start), "use requires a path string"));
                };
                let span = parser.span(start);
                document.uses.push(crate::Use { path, span });
            }
            Kind::Identifier("component") => {
                let component = parser.component()?;
                document.components.push(component);
            }
            _ => {
                let (name, start) = parser.identifier()?;
                document.root = Some(parser.node(name, start, 1)?);
            }
        }
        parser.separators()?;
    }
    Ok(document)
}

pub(crate) struct Parser<'a, 'l> {
    lexer: Lexer<'a>,
    pub current: Token<'a>,
    last_end: usize,
    pub limits: &'l Limits,
    nodes: usize,
}

impl<'a> Parser<'a, '_> {
    pub fn advance(&mut self) -> Result<Token<'a>, Error> {
        let next = self.lexer.next()?;
        self.last_end = self.current.span.end;
        Ok(std::mem::replace(&mut self.current, next))
    }

    pub fn error(&self, message: &str) -> Error {
        Error::new(self.current.span, message)
    }

    /// The range from `start` to the end of the last consumed token.
    pub fn span(&self, start: usize) -> Span {
        Span {
            start,
            end: self.last_end,
        }
    }

    pub fn expect(&mut self, punct: &str, message: &str) -> Result<(), Error> {
        if !matches!(self.current.kind, Kind::Punct(p) if p == punct) {
            return Err(self.error(message));
        }
        self.advance()?;
        Ok(())
    }

    fn separators(&mut self) -> Result<(), Error> {
        while matches!(self.current.kind, Kind::Separator) {
            self.advance()?;
        }
        Ok(())
    }

    pub fn identifier(&mut self) -> Result<(String, usize), Error> {
        match self.current.kind {
            Kind::Identifier(name) => {
                let result = (name.to_owned(), self.current.span.start);
                self.advance()?;
                Ok(result)
            }
            _ => Err(self.error("expected an ASCII component or property name")),
        }
    }

    fn open(&mut self, message: &str) -> Result<(), Error> {
        if !matches!(self.current.kind, Kind::Open) {
            return Err(self.error(message));
        }
        self.advance()?;
        self.separators()
    }

    /// Ends a declaration: `}` follows, separators follow, or a block just closed.
    fn end_declaration(&mut self, block: bool) -> Result<(), Error> {
        match self.current.kind {
            Kind::Close => Ok(()),
            Kind::Separator => self.separators(),
            _ if block => Ok(()),
            _ => Err(self.error("expected a newline, ';' or '}' after the declaration")),
        }
    }

    /// Skips separators after a block and consumes a following `else`.
    fn else_keyword(&mut self) -> Result<bool, Error> {
        self.separators()?;
        if matches!(self.current.kind, Kind::Identifier("else")) {
            self.advance()?;
            return Ok(true);
        }
        Ok(false)
    }

    fn component(&mut self) -> Result<Component, Error> {
        let start = self.current.span.start;
        self.advance()?;
        let (name, _) = self.identifier()?;
        self.expect("(", "expected '(' after the component name")?;
        let mut params = Vec::new();
        while !matches!(self.current.kind, Kind::Punct(")")) {
            let (name, start) = self.identifier()?;
            if !matches!(self.current.kind, Kind::Colon) {
                return Err(self.error("expected ':' and a parameter type"));
            }
            self.advance()?;
            let ty = self.ty()?;
            let default = if matches!(self.current.kind, Kind::Punct("=")) {
                self.advance()?;
                Some(self.expr()?)
            } else {
                None
            };
            params.push(Param {
                name,
                ty,
                default,
                span: self.span(start),
            });
            if !matches!(self.current.kind, Kind::Punct(")")) {
                self.expect(",", "expected ',' or ')' after the parameter")?;
            }
        }
        self.advance()?;
        self.open("expected '{' to start the component body")?;
        let mut states = Vec::new();
        let mut root = None;
        while !matches!(self.current.kind, Kind::Close) {
            if matches!(self.current.kind, Kind::Identifier("state")) {
                states.push(self.state()?);
                self.end_declaration(false)?;
            } else if root.is_none() {
                let (name, start) = self.identifier()?;
                root = Some(self.node(name, start, 1)?);
                self.end_declaration(true)?;
            } else {
                return Err(self.error("a component body has exactly one root node"));
            }
        }
        self.advance()?;
        let mut root =
            root.ok_or_else(|| Error::new(self.span(start), "component has no root node"))?;
        states.append(&mut root.states);
        root.states = states;
        Ok(Component {
            name,
            params,
            root,
            span: self.span(start),
        })
    }

    fn state(&mut self) -> Result<State, Error> {
        let start = self.current.span.start;
        self.advance()?;
        let (name, _) = self.identifier()?;
        if !matches!(self.current.kind, Kind::Colon) {
            return Err(self.error("expected ':' and a state type"));
        }
        self.advance()?;
        let ty = self.ty()?;
        self.expect("=", "a state requires '=' and an initial value")?;
        let value = self.expr()?;
        Ok(State {
            name,
            ty,
            value,
            span: self.span(start),
        })
    }

    fn check_budget(&mut self, depth: usize) -> Result<(), Error> {
        if depth > self.limits.max_depth {
            return Err(self.error("component nesting exceeds its depth limit"));
        }
        Ok(())
    }

    fn node(&mut self, name: String, start: usize, depth: usize) -> Result<Node, Error> {
        self.check_budget(depth)?;
        if self.nodes >= self.limits.max_nodes {
            return Err(self.error("component count exceeds its node limit"));
        }
        self.nodes += 1;
        self.open("expected '{' after the component name")?;
        let mut node = Node {
            name,
            properties: Vec::new(),
            states: Vec::new(),
            events: Vec::new(),
            children: Vec::new(),
            span: Span { start, end: start },
        };
        while !matches!(self.current.kind, Kind::Close) {
            if matches!(self.current.kind, Kind::End) {
                return Err(self.error("expected '}' to close the component"));
            }
            let block = match self.current.kind {
                Kind::Identifier("state") => {
                    node.states.push(self.state()?);
                    false
                }
                Kind::Identifier("on") => {
                    let start = self.current.span.start;
                    self.advance()?;
                    let (name, _) = self.identifier()?;
                    let body = self.statements(depth + 1)?;
                    node.events.push(Event {
                        name,
                        body,
                        span: self.span(start),
                    });
                    true
                }
                Kind::Identifier("if" | "for") => {
                    node.children.push(self.item(depth + 1)?);
                    true
                }
                _ => {
                    let (name, start) = self.identifier()?;
                    match self.current.kind {
                        Kind::Open => {
                            node.children
                                .push(Item::Node(self.node(name, start, depth + 1)?));
                            false
                        }
                        Kind::Colon => {
                            self.advance()?;
                            let value_start = self.current.span.start;
                            let value = Value::from(self.expr()?);
                            let value_span = self.span(value_start);
                            node.properties.push(Property {
                                name,
                                value,
                                span: self.span(start),
                                value_span,
                            });
                            false
                        }
                        _ => {
                            return Err(
                                self.error("expected ':' for a property or '{' for a child")
                            );
                        }
                    }
                }
            };
            self.end_declaration(block)?;
        }
        node.span.end = self.current.span.end;
        self.advance()?;
        Ok(node)
    }

    /// Parses an `if` or `for` block of child items.
    fn item(&mut self, depth: usize) -> Result<Item, Error> {
        self.check_budget(depth)?;
        let keyword = self.advance()?.kind;
        if matches!(keyword, Kind::Identifier("for")) {
            let (name, _) = self.identifier()?;
            if !matches!(self.current.kind, Kind::Identifier("in")) {
                return Err(self.error("expected 'in' after the loop variable"));
            }
            self.advance()?;
            let list = self.expr()?;
            let body = self.items(depth)?;
            return Ok(Item::For(name, list, body));
        }
        let condition = self.expr()?;
        let then = self.items(depth)?;
        let otherwise = if !self.else_keyword()? {
            Vec::new()
        } else if matches!(self.current.kind, Kind::Identifier("if")) {
            vec![self.item(depth + 1)?]
        } else {
            self.items(depth)?
        };
        Ok(Item::If(condition, then, otherwise))
    }

    /// Parses `{ child nodes and blocks }`.
    fn items(&mut self, depth: usize) -> Result<Vec<Item>, Error> {
        self.open("expected '{' to start the block")?;
        let mut items = Vec::new();
        while !matches!(self.current.kind, Kind::Close) {
            let block = if matches!(self.current.kind, Kind::Identifier("if" | "for")) {
                items.push(self.item(depth + 1)?);
                true
            } else {
                let (name, start) = self.identifier()?;
                items.push(Item::Node(self.node(name, start, depth + 1)?));
                false
            };
            self.end_declaration(block)?;
        }
        self.advance()?;
        Ok(items)
    }

    /// Parses `{ statements }` of an event handler.
    fn statements(&mut self, depth: usize) -> Result<Vec<Statement>, Error> {
        self.check_budget(depth)?;
        self.open("expected '{' to start the statements")?;
        let mut statements = Vec::new();
        while !matches!(self.current.kind, Kind::Close) {
            let start = self.current.span.start;
            let block = if matches!(self.current.kind, Kind::Identifier("if")) {
                self.advance()?;
                let condition = self.expr()?;
                let then = self.statements(depth + 1)?;
                let otherwise = if !self.else_keyword()? {
                    Vec::new()
                } else if matches!(self.current.kind, Kind::Identifier("if")) {
                    // `else if` nests a single conditional statement.
                    self.statements_from_if(depth + 1)?
                } else {
                    self.statements(depth + 1)?
                };
                statements.push(Statement::If(condition, then, otherwise));
                true
            } else {
                let (target, _) = self.identifier()?;
                let operator = match self.current.kind {
                    Kind::Punct(operator @ ("=" | "+=" | "-=")) => operator,
                    _ => return Err(self.error("expected '=', '+=' or '-=' in an assignment")),
                };
                self.advance()?;
                let value = self.expr()?;
                statements.push(Statement::Assign {
                    target,
                    operator,
                    value,
                    span: self.span(start),
                });
                false
            };
            self.end_declaration(block)?;
        }
        self.advance()?;
        Ok(statements)
    }

    fn statements_from_if(&mut self, depth: usize) -> Result<Vec<Statement>, Error> {
        self.check_budget(depth)?;
        self.advance()?;
        let condition = self.expr()?;
        let then = self.statements(depth + 1)?;
        let otherwise = if !self.else_keyword()? {
            Vec::new()
        } else if matches!(self.current.kind, Kind::Identifier("if")) {
            self.statements_from_if(depth + 1)?
        } else {
            self.statements(depth + 1)?
        };
        Ok(vec![Statement::If(condition, then, otherwise)])
    }
}
