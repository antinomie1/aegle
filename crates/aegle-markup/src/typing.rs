//! Expression name resolution and typing.

use crate::program::{Checker, Scope};
use crate::{Error, Expr, ExprKind, Kind, Ref, Type, Value};

impl Checker {
    /// Resolves names in place and returns the type, coerced to `expected`.
    /// Only an integer literal converts implicitly, and only to float.
    pub(crate) fn expr(
        &mut self,
        expr: &mut Expr,
        scope: &Scope,
        expected: Option<&Type>,
    ) -> Result<Type, Error> {
        let found = self.infer(expr, scope, expected)?;
        match expected {
            Some(expected) => coerce(expr, found, expected),
            None => Ok(found),
        }
    }

    fn infer(
        &mut self,
        expr: &mut Expr,
        scope: &Scope,
        expected: Option<&Type>,
    ) -> Result<Type, Error> {
        let span = expr.span;
        let error = |message: String| Err(Error::new(span, message));
        Ok(match &mut expr.kind {
            ExprKind::Literal(Value::String(_)) => Type::String,
            ExprKind::Literal(Value::Bool(_)) => Type::Bool,
            ExprKind::Literal(Value::Int(_)) => Type::Int,
            ExprKind::Literal(Value::Number(_)) => Type::Float,
            ExprKind::Literal(_) => {
                return error("expressions accept bool, int, float, string and list values".into());
            }
            ExprKind::Name(name) => {
                let (reference, ty) =
                    if let Some(i) = scope.items.iter().rposition(|(n, _)| n == name) {
                        (Ref::Item(i), &scope.items[i].1)
                    } else if let Some(i) = scope.states.iter().position(|(n, _)| n == name) {
                        (Ref::State(i), &scope.states[i].1)
                    } else if let Some(i) = scope.params.iter().position(|(n, _)| n == name) {
                        (Ref::Param(i), &scope.params[i].1)
                    } else {
                        return error(format!("unknown name `{name}`"));
                    };
                let ty = ty.clone();
                expr.kind = ExprKind::Ref(reference);
                ty
            }
            ExprKind::SelfField(field) => match (scope.source, field.as_str()) {
                (Some(Kind::CheckBox | Kind::Switch), "checked") => Type::Bool,
                (Some(Kind::CheckBox | Kind::Switch | Kind::TextField), "text") => Type::String,
                (Some(Kind::Slider), "value") => Type::Float,
                (None, _) => return error("self is only available in event handlers".into()),
                (Some(kind), field) => return error(format!("{kind:?} has no field `{field}`")),
            },
            ExprKind::Unary(operator, operand) => {
                if *operator == "!" {
                    self.expr(operand, scope, Some(&Type::Bool))?
                } else {
                    let ty = self.expr(operand, scope, None)?;
                    if !matches!(ty, Type::Int | Type::Float) {
                        return error("`-` requires an int or float".into());
                    }
                    ty
                }
            }
            ExprKind::Binary(operator, left, right) => {
                let operator = *operator;
                if matches!(operator, "&&" | "||") {
                    self.expr(left, scope, Some(&Type::Bool))?;
                    self.expr(right, scope, Some(&Type::Bool))?;
                    return Ok(Type::Bool);
                }
                let ty = self.unify(left, right, scope)?;
                let fits = match operator {
                    "==" | "!=" => true,
                    "<" | "<=" | ">" | ">=" => matches!(ty, Type::Int | Type::Float | Type::String),
                    "+" => !matches!(ty, Type::Bool),
                    _ => matches!(ty, Type::Int | Type::Float),
                };
                if !fits {
                    return error(format!("`{operator}` does not apply to {}", name(&ty)));
                }
                if matches!(operator, "==" | "!=" | "<" | "<=" | ">" | ">=") {
                    Type::Bool
                } else {
                    ty
                }
            }
            ExprKind::Call(function, arguments) => {
                let [argument] = arguments.as_mut_slice() else {
                    return error(format!("`{function}` takes one argument"));
                };
                let ty = self.expr(argument, scope, None)?;
                match (function.as_str(), ty) {
                    ("str", Type::Bool | Type::Int | Type::Float | Type::String) => Type::String,
                    ("len", Type::List(_)) => Type::Int,
                    ("int", Type::Float) => Type::Int,
                    ("float", Type::Int) => Type::Float,
                    ("str" | "len" | "int" | "float", ty) => {
                        return error(format!("`{function}` does not accept {}", name(&ty)));
                    }
                    _ => return error(format!("unknown function `{function}`")),
                }
            }
            ExprKind::List(items) => {
                let item = match (items.first_mut(), expected) {
                    (None, Some(Type::List(item))) => return Ok(Type::List(item.clone())),
                    (None, _) => return error("the type of an empty list must be known".into()),
                    (Some(first), expected) => {
                        let hint = match expected {
                            Some(Type::List(item)) => Some(&**item),
                            _ => None,
                        };
                        self.expr(first, scope, hint)?
                    }
                };
                if !matches!(item, Type::Int | Type::String) {
                    return error("list items must be int or string".into());
                }
                for value in &mut items[1..] {
                    self.expr(value, scope, Some(&item))?;
                }
                Type::List(Box::new(item))
            }
            ExprKind::Ref(_) => return error("unexpected pre-resolved reference".into()),
        })
    }

    /// Types both operands alike, converting one integer literal against a float.
    fn unify(&mut self, left: &mut Expr, right: &mut Expr, scope: &Scope) -> Result<Type, Error> {
        let left_ty = self.expr(left, scope, None)?;
        if matches!(&right.kind, ExprKind::List(items) if items.is_empty()) {
            return self.expr(right, scope, Some(&left_ty));
        }
        let right_ty = self.expr(right, scope, None)?;
        match (left_ty, right_ty) {
            (left_ty, right_ty) if left_ty == right_ty => Ok(left_ty),
            (Type::Int, Type::Float) => coerce(left, Type::Int, &Type::Float),
            (left_ty, right_ty) => coerce(right, right_ty, &left_ty),
        }
    }
}

fn coerce(expr: &mut Expr, found: Type, expected: &Type) -> Result<Type, Error> {
    if &found == expected {
        return Ok(found);
    }
    if let (Type::Float, ExprKind::Literal(Value::Int(n))) = (expected, &expr.kind) {
        expr.kind = ExprKind::Literal(Value::Number(*n as f32));
        return Ok(Type::Float);
    }
    Err(Error::new(
        expr.span,
        format!("expected {}, found {}", name(expected), name(&found)),
    ))
}

fn name(ty: &Type) -> String {
    match ty {
        Type::Bool => "bool".into(),
        Type::Int => "int".into(),
        Type::Float => "float".into(),
        Type::String => "string".into(),
        Type::List(item) => format!("list<{}>", name(item)),
    }
}
