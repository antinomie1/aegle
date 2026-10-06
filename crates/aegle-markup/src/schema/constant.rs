//! Constant property values: lists of literals and bare identifiers, and the
//! layout functions `repeat`, `minmax`, `fit_content` and `calc`.

use crate::{Expr, ExprKind, Value};

/// Integer literals are numbers wherever a static property expects one, and
/// a constant expression is its constant value.
pub(crate) fn literal(value: Value) -> Value {
    match value {
        Value::Int(n) => Value::Number(n as f32),
        Value::Expr(expr) => constant(&expr).unwrap_or(Value::Expr(expr)),
        value => value,
    }
}

/// A `[...]` list or layout function of literals, bare identifiers and
/// further constants as a value; `None` for any other expression.
pub(crate) fn constant(expr: &Expr) -> Option<Value> {
    match &expr.kind {
        ExprKind::List(items) => items
            .iter()
            .map(item)
            .collect::<Option<_>>()
            .map(Value::List),
        ExprKind::Call(name, arguments) if name == "calc" => {
            let [argument] = &arguments[..] else {
                return None;
            };
            let (percent, length) = linear(argument)?;
            let parts = vec![Value::Percent(percent), Value::Length(length)];
            Some(Value::Call(name.clone(), parts))
        }
        ExprKind::Call(name, arguments)
            if matches!(name.as_str(), "repeat" | "minmax" | "fit_content") =>
        {
            let arguments = arguments.iter().map(item).collect::<Option<_>>()?;
            Some(Value::Call(name.clone(), arguments))
        }
        _ => None,
    }
}

fn item(expr: &Expr) -> Option<Value> {
    match &expr.kind {
        ExprKind::Literal(value) => Some(literal(value.clone())),
        ExprKind::Name(name) => Some(Value::Identifier(name.clone())),
        _ => constant(expr),
    }
}

/// A sum of percentages and lengths, each optionally scaled by a number,
/// as `(percent, length)`.
fn linear(expr: &Expr) -> Option<(f32, f32)> {
    let number = |expr: &Expr| match expr.kind {
        ExprKind::Literal(Value::Int(n)) => Some(n as f32),
        ExprKind::Literal(Value::Number(n)) => Some(n),
        _ => None,
    };
    let scale = |(percent, length): (f32, f32), k: f32| (percent * k, length * k);
    Some(match &expr.kind {
        ExprKind::Literal(Value::Percent(p)) => (*p, 0.0),
        ExprKind::Literal(Value::Length(l)) => (0.0, *l),
        ExprKind::Unary("-", inner) => scale(linear(inner)?, -1.0),
        ExprKind::Binary(operator @ ("+" | "-"), left, right) => {
            let sign = if *operator == "+" { 1.0 } else { -1.0 };
            let (left, right) = (linear(left)?, linear(right)?);
            (left.0 + sign * right.0, left.1 + sign * right.1)
        }
        ExprKind::Binary("*", left, right) => match (number(left), number(right)) {
            (Some(k), None) => scale(linear(right)?, k),
            (None, Some(k)) => scale(linear(left)?, k),
            _ => return None,
        },
        ExprKind::Binary("/", left, right) => scale(linear(left)?, 1.0 / number(right)?),
        _ => return None,
    })
}
