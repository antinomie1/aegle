//! Event handler statements: assignments, conditionals, locals, host calls and emits.

use super::Parser;
use crate::lexer::Kind;
use crate::{Error, Statement};

impl Parser<'_, '_> {
    /// Parses `{ statements }` of an event handler.
    pub(super) fn statements(&mut self, depth: usize) -> Result<Vec<Statement>, Error> {
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
            } else if matches!(self.current.kind, Kind::Identifier("let")) {
                self.advance()?;
                let (name, _) = self.identifier()?;
                self.expect("=", "a let requires '=' and a value")?;
                let value = self.expr()?;
                statements.push(Statement::Let {
                    name,
                    value,
                    span: self.span(start),
                });
                false
            } else if matches!(self.current.kind, Kind::Identifier("emit")) {
                self.advance()?;
                let (name, _) = self.identifier()?;
                let value = if matches!(self.current.kind, Kind::Punct("(")) {
                    self.advance()?;
                    let value = self.expr()?;
                    self.expect(")", "expected ')' after the emitted value")?;
                    Some(value)
                } else {
                    None
                };
                statements.push(Statement::Emit {
                    name,
                    value,
                    span: self.span(start),
                });
                false
            } else {
                let (target, _) = self.identifier()?;
                if target == "host" && matches!(self.current.kind, Kind::Punct(".")) {
                    self.advance()?;
                    let (name, _) = self.identifier()?;
                    self.expect("(", "expected '(' after the host action name")?;
                    let arguments = self.list(")", depth)?;
                    statements.push(Statement::Host {
                        name,
                        arguments,
                        span: self.span(start),
                    });
                } else {
                    let operator = match self.current.kind {
                        Kind::Punct(operator @ ("=" | "+=" | "-=")) => operator,
                        _ => {
                            return Err(self.error("expected '=', '+=' or '-=' in an assignment"));
                        }
                    };
                    self.advance()?;
                    let value = self.expr()?;
                    statements.push(Statement::Assign {
                        target,
                        operator,
                        value,
                        span: self.span(start),
                    });
                }
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
