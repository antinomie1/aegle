//! Event statements: assignments, conditionals, locals, host calls and emits.

use super::{Checker, Scope};
use crate::checked::{HostCall, Step};
use crate::schema::valid_id;
use crate::{Error, Statement, Type};

impl Checker {
    pub(super) fn steps(
        &mut self,
        statements: Vec<Statement>,
        scope: &mut Scope,
    ) -> Result<Vec<Step>, Error> {
        let depth = scope.locals.len();
        let result = self.steps_in(statements, scope);
        scope.locals.truncate(depth);
        result
    }

    fn steps_in(
        &mut self,
        statements: Vec<Statement>,
        scope: &mut Scope,
    ) -> Result<Vec<Step>, Error> {
        let mut steps = Vec::new();
        for statement in statements {
            steps.push(match statement {
                Statement::Assign {
                    target,
                    operator,
                    mut value,
                    span,
                } => {
                    let index = scope
                        .states
                        .iter()
                        .position(|(name, _)| *name == target)
                        .ok_or_else(|| {
                            Error::new(
                                span,
                                format!("`{target}` is not a state of this document or component"),
                            )
                        })?;
                    let ty = scope.states[index].1.clone();
                    let numeric = matches!(ty, Type::Int | Type::Float);
                    let appendable = matches!(ty, Type::String | Type::List(_));
                    if operator != "=" && !(numeric || (operator == "+=" && appendable)) {
                        return Err(Error::new(
                            span,
                            format!("`{operator}` does not apply to this state"),
                        ));
                    }
                    self.expr(&mut value, scope, Some(&ty))?;
                    Step::Assign(index, operator, value)
                }
                Statement::If(mut condition, then, otherwise) => {
                    self.expr(&mut condition, scope, Some(&Type::Bool))?;
                    Step::If(
                        condition,
                        self.steps(then, scope)?,
                        self.steps(otherwise, scope)?,
                    )
                }
                Statement::Let {
                    name,
                    mut value,
                    span,
                } => {
                    if !valid_id(&name) {
                        return Err(Error::new(span, "local names must be Rust identifiers"));
                    }
                    let ty = self.expr(&mut value, scope, None)?;
                    scope.locals.push((name, ty));
                    Step::Let(value)
                }
                Statement::Host {
                    name,
                    arguments,
                    span,
                } => {
                    let mut checked = Vec::new();
                    let mut types = Vec::new();
                    for mut argument in arguments {
                        types.push(self.expr(&mut argument, scope, None)?);
                        checked.push(argument);
                    }
                    self.host_calls.push(HostCall {
                        name: name.clone(),
                        types,
                        file: self.file,
                        span,
                    });
                    Step::Host(name, checked, span)
                }
                Statement::Emit { name, value, span } => {
                    let sigs = &self.event_sigs[scope.template];
                    let index = sigs
                        .iter()
                        .position(|(event, _)| *event == name)
                        .ok_or_else(|| {
                            Error::new(span, format!("this component declares no event `{name}`"))
                        })?;
                    let expected = sigs[index].1.clone();
                    let value = match (value, expected) {
                        (Some(mut value), Some(ty)) => {
                            self.expr(&mut value, scope, Some(&ty))?;
                            Some(value)
                        }
                        (None, None) => None,
                        (Some(_), None) => {
                            return Err(Error::new(span, "this event carries no value"));
                        }
                        (None, Some(_)) => {
                            return Err(Error::new(span, "this event requires a value"));
                        }
                    };
                    Step::Emit(index, value)
                }
            });
        }
        Ok(steps)
    }
}
