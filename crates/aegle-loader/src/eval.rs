//! Expression evaluation and event statements over checked programs.

use std::rc::Rc;

use aegle_markup::{Child, Expr, ExprKind, Ref, Span, Step, Type, Value};
use aegle_ui::Result;

use crate::{
    Data, RuntimeError, Shared,
    handle::Handle,
    reactive::{Cell, Effect},
};

/// A component argument: an expression in the caller's environment, or a default.
pub(crate) enum Param {
    Bound(Rc<Expr>, Env),
    Value(Data),
}

/// A component event handler, run in the caller's environment.
pub(crate) struct Emit {
    pub steps: Rc<[Step]>,
    pub binds_value: bool,
    pub env: Env,
}

/// The children of a component instance, built where its body writes `slot`.
pub(crate) struct Slot {
    pub children: Rc<[Child]>,
    pub env: Env,
}

/// The states, parameters and event handlers of one document or component instance.
pub(crate) struct Scope {
    pub template: usize,
    pub states: Vec<Rc<Cell>>,
    pub params: Vec<Param>,
    /// One entry per declared event of the template.
    pub emits: Vec<Option<Emit>>,
}

/// Everything an expression can read.
#[derive(Clone)]
pub(crate) struct Env {
    pub shared: Rc<Shared>,
    pub scope: Rc<Scope>,
    /// Items of the enclosing `for` blocks, outermost first.
    pub items: Rc<[Data]>,
    /// Children passed to the component instance this environment belongs to.
    pub slot: Option<Rc<Slot>>,
}

/// What a handler's expressions read besides the environment: the control
/// that raised the event and the `let` locals.
pub(crate) struct Frame<'a> {
    pub source: &'a Handle,
    pub locals: &'a [Data],
}

/// Statement budget and event nesting of one handler run.
pub(crate) struct Run {
    pub locals: Vec<Data>,
    pub steps: usize,
    pub depth: usize,
}

impl Env {
    /// Creates an instance of `template`: parameters, then states in order.
    /// `carried` may supply a state's value instead of its initializer.
    pub fn instantiate(
        shared: Rc<Shared>,
        template: usize,
        params: Vec<Param>,
        emits: Vec<Option<Emit>>,
        carried: &dyn Fn(&str, &Type) -> Option<Data>,
    ) -> Result<Self> {
        let states = &shared.checked.templates[template].states;
        let cells = states
            .iter()
            .map(|_| Cell::new(Data::Bool(false)))
            .collect();
        let env = Self {
            shared: shared.clone(),
            scope: Rc::new(Scope {
                template,
                states: cells,
                params,
                emits,
            }),
            items: Rc::from([]),
            slot: None,
        };
        // Initializers read only parameters and earlier states.
        for (index, (name, ty, initial)) in states.iter().enumerate() {
            let value = match carried(name, ty) {
                Some(value) => value,
                None => eval(initial, &env, None, None).map_err(|e| env.locate(e))?,
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

    /// Names the file of this environment's template in a runtime error.
    pub fn locate(&self, error: Box<dyn std::error::Error>) -> Box<dyn std::error::Error> {
        let program = &self.shared.checked;
        match error.downcast::<RuntimeError>() {
            Ok(mut error) if error.file.is_none() => {
                let file = program.templates[self.scope.template].file;
                error.file = program.files.get(file).cloned();
                error
            }
            Ok(error) => error,
            Err(other) => other,
        }
    }
}

/// Evaluates a checked expression. Reads subscribe `effect`; `frame` serves
/// `self` fields and locals inside event handlers.
pub(crate) fn eval(
    expr: &Expr,
    env: &Env,
    effect: Option<&Rc<Effect>>,
    frame: Option<&Frame>,
) -> Result<Data> {
    let eval = |expr: &Expr| eval(expr, env, effect, frame);
    Ok(match &expr.kind {
        ExprKind::Literal(value) => literal(value),
        ExprKind::Ref(Ref::State(index)) => env.scope.states[*index].get(effect),
        ExprKind::Ref(Ref::Param(index)) => match &env.scope.params[*index] {
            Param::Bound(expr, caller) => crate::eval::eval(expr, caller, effect, None)?,
            Param::Value(value) => value.clone(),
        },
        ExprKind::Ref(Ref::Item(index)) => env.items[*index].clone(),
        ExprKind::Ref(Ref::Local(index)) => {
            frame.expect("checked: locals only in handlers").locals[*index].clone()
        }
        ExprKind::SelfField(field) => frame
            .expect("checked: self only in handlers")
            .source
            .field(field),
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
        ExprKind::Record(_, values) => {
            Data::Record(values.iter().map(eval).collect::<Result<Vec<_>>>()?.into())
        }
        ExprKind::FieldAt(record, index) => match eval(record)? {
            Data::Record(fields) => fields[*index].clone(),
            _ => unreachable!("checked record field"),
        },
        ExprKind::Name(_) | ExprKind::Field(..) => {
            unreachable!("checked expressions are resolved")
        }
    })
}

/// Runs a handler's statements. An error stops it and keeps earlier assignments.
pub(crate) fn handle(steps: &[Step], env: &Env, source: &Handle) -> Result {
    let mut run = Run {
        locals: Vec::new(),
        steps: env.shared.limits.get().steps,
        depth: 0,
    };
    exec(steps, env, source, &mut run).map_err(|error| env.locate(error))
}

/// Runs event statements; each assignment updates dependent bindings at once.
fn exec(steps: &[Step], env: &Env, source: &Handle, run: &mut Run) -> Result {
    let mark = run.locals.len();
    let result = run_steps(steps, env, source, run);
    run.locals.truncate(mark);
    result
}

fn run_steps(steps: &[Step], env: &Env, source: &Handle, run: &mut Run) -> Result {
    for step in steps {
        run.steps = run.steps.checked_sub(1).ok_or_else(|| {
            RuntimeError::new(Span::default(), "the handler exceeded its statement limit")
        })?;
        let value_of = |expr: &Expr, run: &Run| {
            let frame = Frame {
                source,
                locals: &run.locals,
            };
            eval(expr, env, None, Some(&frame))
        };
        match step {
            Step::Assign(state, operator, expr) => {
                let value = value_of(expr, run)?;
                let cell = &env.scope.states[*state];
                let value = match *operator {
                    "=" => value,
                    "+=" => binary("+", cell.get(None), value, expr.span)?,
                    _ => binary("-", cell.get(None), value, expr.span)?,
                };
                cell.set(value)?;
            }
            Step::If(condition, then, otherwise) => {
                let taken = truth(value_of(condition, run)?);
                exec(if taken { then } else { otherwise }, env, source, run)?;
            }
            Step::Let(expr) => {
                let value = value_of(expr, run)?;
                run.locals.push(value);
            }
            Step::Host(name, arguments, span) => {
                let values = arguments
                    .iter()
                    .map(|argument| value_of(argument, run))
                    .collect::<Result<Vec<_>>>()?;
                env.shared.actions.call(name, &values, *span)?;
            }
            Step::Emit(index, value) => {
                let value = value.as_ref().map(|v| value_of(v, run)).transpose()?;
                let Some(emit) = &env.scope.emits[*index] else {
                    continue;
                };
                if run.depth >= env.shared.limits.get().emit_depth {
                    let message = "component events nest deeper than the emit limit";
                    return Err(RuntimeError::new(Span::default(), message).into());
                }
                let mut inner = Run {
                    locals: value.filter(|_| emit.binds_value).into_iter().collect(),
                    steps: run.steps,
                    depth: run.depth + 1,
                };
                let result = exec(&emit.steps, &emit.env, source, &mut inner)
                    .map_err(|error| emit.env.locate(error));
                run.steps = inner.steps;
                result?;
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
