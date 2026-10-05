//! Expression evaluation and event statements over checked programs.

use std::rc::Rc;

use aegle_app::Result;
use aegle_markup::{Expr, ExprKind, Ref, Span, Step, Type, Value};

use crate::{Data, RuntimeError, handle::Handle, reactive::Cell, reactive::Effect};

/// A component argument: an expression in the caller's environment, or a default.
pub(crate) enum Param {
    Bound(Rc<Expr>, Env),
    Value(Data),
}

/// The states and parameters of one document or component instance.
pub(crate) struct Scope {
    pub states: Vec<Rc<Cell>>,
    pub params: Vec<Param>,
}

/// Everything an expression can read.
#[derive(Clone)]
pub(crate) struct Env {
    pub program: Rc<aegle_markup::Program>,
    pub scope: Rc<Scope>,
    /// Items of the enclosing `for` blocks, outermost first.
    pub items: Rc<[Data]>,
}

impl Env {
    /// Creates an instance of `template`: parameters, then states in order.
    /// `carried` may supply a state's value instead of its initializer.
    pub fn instantiate(
        program: Rc<aegle_markup::Program>,
        template: usize,
        params: Vec<Param>,
        carried: &dyn Fn(&str, &Type) -> Option<Data>,
    ) -> Result<Self> {
        let states = &program.templates[template].states;
        let cells = states
            .iter()
            .map(|_| Cell::new(Data::Bool(false)))
            .collect();
        let env = Self {
            program: program.clone(),
            scope: Rc::new(Scope {
                states: cells,
                params,
            }),
            items: Rc::from([]),
        };
        // Initializers read only parameters and earlier states.
        for (index, (name, ty, initial)) in states.iter().enumerate() {
            let value = match carried(name, ty) {
                Some(value) => value,
                None => eval(initial, &env, None, None)?,
            };
            env.scope.states[index].init(value);
        }
        Ok(env)
    }

    pub fn with_item(&self, item: Data) -> Self {
        let mut items = self.items.to_vec();
        items.push(item);
        Self {
            items: items.into(),
            ..self.clone()
        }
    }
}

/// Evaluates a checked expression. Reads subscribe `effect`; `source` serves
/// `self` fields inside event handlers.
pub(crate) fn eval(
    expr: &Expr,
    env: &Env,
    effect: Option<&Rc<Effect>>,
    source: Option<&Handle>,
) -> Result<Data> {
    let eval = |expr: &Expr| eval(expr, env, effect, source);
    Ok(match &expr.kind {
        ExprKind::Literal(value) => literal(value),
        ExprKind::Ref(Ref::State(index)) => env.scope.states[*index].get(effect),
        ExprKind::Ref(Ref::Param(index)) => match &env.scope.params[*index] {
            Param::Bound(expr, caller) => crate::eval::eval(expr, caller, effect, None)?,
            Param::Value(value) => value.clone(),
        },
        ExprKind::Ref(Ref::Item(index)) => env.items[*index].clone(),
        ExprKind::SelfField(field) => source
            .expect("checked: self only in handlers")
            .field(field)?,
        ExprKind::Unary("!", operand) => Data::Bool(!truth(eval(operand)?)),
        ExprKind::Unary(_, operand) => match eval(operand)? {
            Data::Int(n) => Data::Int(n.checked_neg().ok_or_else(|| overflow(expr.span))?),
            Data::Float(x) => Data::Float(-x),
            _ => unreachable!("checked numeric negation"),
        },
        ExprKind::Binary("&&", left, right) => {
            Data::Bool(truth(eval(left)?) && truth(eval(right)?))
        }
        ExprKind::Binary("||", left, right) => {
            Data::Bool(truth(eval(left)?) || truth(eval(right)?))
        }
        ExprKind::Binary(operator, left, right) => {
            binary(operator, eval(left)?, eval(right)?, expr.span)?
        }
        ExprKind::Call(function, arguments) => call(function, eval(&arguments[0])?, expr.span)?,
        ExprKind::List(items) => {
            Data::List(items.iter().map(eval).collect::<Result<Vec<_>>>()?.into())
        }
        ExprKind::Name(_) => unreachable!("checked expressions are resolved"),
    })
}

/// Runs event statements; each assignment updates dependent bindings at once.
/// An error stops the handler and keeps earlier assignments.
pub(crate) fn exec(steps: &[Step], env: &Env, source: &Handle) -> Result {
    for step in steps {
        match step {
            Step::Assign(state, operator, expr) => {
                let value = eval(expr, env, None, Some(source))?;
                let cell = &env.scope.states[*state];
                let value = match *operator {
                    "=" => value,
                    "+=" => binary("+", cell.get(None), value, expr.span)?,
                    _ => binary("-", cell.get(None), value, expr.span)?,
                };
                cell.set(value)?;
            }
            Step::If(condition, then, otherwise) => {
                let taken = truth(eval(condition, env, None, Some(source))?);
                exec(if taken { then } else { otherwise }, env, source)?;
            }
        }
    }
    Ok(())
}

fn literal(value: &Value) -> Data {
    match value {
        Value::String(text) => Data::String(text.as_str().into()),
        Value::Bool(value) => Data::Bool(*value),
        Value::Int(value) => Data::Int(*value),
        Value::Number(value) => Data::Float(*value),
        _ => unreachable!("checked expression literal"),
    }
}

pub(crate) fn truth(value: Data) -> bool {
    matches!(value, Data::Bool(true))
}

fn overflow(span: Span) -> RuntimeError {
    RuntimeError::new(span, "integer overflow or division by zero")
}

fn binary(operator: &str, left: Data, right: Data, span: Span) -> Result<Data> {
    use Data::*;
    let ordering = match (&left, &right) {
        (Int(a), Int(b)) => a.partial_cmp(b),
        (Float(a), Float(b)) => a.partial_cmp(b),
        (String(a), String(b)) => a.partial_cmp(b),
        _ => None,
    };
    Ok(match (operator, left, right) {
        ("==", left, right) => Bool(left == right),
        ("!=", left, right) => Bool(left != right),
        ("<", ..) => Bool(ordering.is_some_and(|o| o.is_lt())),
        ("<=", ..) => Bool(ordering.is_some_and(|o| o.is_le())),
        (">", ..) => Bool(ordering.is_some_and(|o| o.is_gt())),
        (">=", ..) => Bool(ordering.is_some_and(|o| o.is_ge())),
        (operator, Int(a), Int(b)) => Int(match operator {
            "+" => a.checked_add(b),
            "-" => a.checked_sub(b),
            "*" => a.checked_mul(b),
            "/" => a.checked_div(b),
            _ => a.checked_rem(b),
        }
        .ok_or_else(|| overflow(span))?),
        (operator, Float(a), Float(b)) => {
            let value = match operator {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a / b,
                _ => a % b,
            };
            if !value.is_finite() {
                return Err(RuntimeError::new(span, "float result is not finite").into());
            }
            Float(value)
        }
        ("+", String(a), String(b)) => String(format!("{a}{b}").into()),
        ("+", List(a), List(b)) => List(a.iter().chain(b.iter()).cloned().collect()),
        _ => unreachable!("checked operator types"),
    })
}

fn call(function: &str, value: Data, span: Span) -> Result<Data> {
    Ok(match (function, value) {
        ("str", Data::Bool(value)) => Data::String(value.to_string().into()),
        ("str", Data::Int(value)) => Data::String(value.to_string().into()),
        ("str", Data::Float(value)) => Data::String(value.to_string().into()),
        ("str", Data::String(value)) => Data::String(value),
        ("len", Data::List(items)) => Data::Int(items.len() as i64),
        ("float", Data::Int(value)) => Data::Float(value as f32),
        ("int", Data::Float(value)) => {
            // 2^63 is exactly representable; i64 holds values below it.
            if !(-9.223_372e18..9.223_372e18).contains(&value) {
                return Err(RuntimeError::new(span, "float is outside the int range").into());
            }
            Data::Int(value as i64)
        }
        _ => unreachable!("checked function arguments"),
    })
}
