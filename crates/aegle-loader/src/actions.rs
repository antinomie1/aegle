//! Host actions and the limits the engine enforces on a program.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use aegle_markup::{Span, Type};
use aegle_ui::{HandlerResult, Result};

use crate::{Data, RuntimeError};

/// Bounds on what one program may do. Zero is a valid restrictive value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Statements one event handler run may execute, counting each assignment,
    /// `if`, `let`, host call and `emit`. Default 10,000.
    pub steps: usize,
    /// Items one `for` block may hold; a longer list is an error and the block
    /// keeps its previous rows. Default 10,000.
    pub rows: usize,
    /// Nested component events: a handler that emits again from inside an
    /// `emit` handler. Default 64.
    pub emit_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 10_000,
            rows: 10_000,
            emit_depth: 64,
        }
    }
}

type Run = Rc<RefCell<dyn FnMut(&[Data]) -> Result>>;

struct Action {
    params: Vec<Type>,
    run: Run,
}

thread_local! {
    /// The host actions of this thread's programs, by name.
    static ACTIONS: RefCell<HashMap<String, Action>> = RefCell::default();
}

/// Registers a host action that `host.name(...)` statements of every program
/// on this thread call, with the markup types of its arguments, replacing an
/// action of that name. Building a view fails, before anything is mounted, if
/// a call has no action or different argument types. The action runs outside
/// every UI borrow, like an event handler; an error it returns stops the
/// handler.
pub fn action<R: HandlerResult>(
    name: &str,
    params: &[Type],
    mut run: impl FnMut(&[Data]) -> R + 'static,
) {
    let action = Action {
        params: params.to_vec(),
        run: Rc::new(RefCell::new(move |arguments: &[Data]| {
            run(arguments).into_result()
        })),
    };
    ACTIONS.with_borrow_mut(|actions| actions.insert(name.to_owned(), action));
}

/// Checks every `host.name(...)` call of `program` against the registry.
pub(crate) fn validate(program: &aegle_markup::Program) -> Result {
    for call in &program.host_calls {
        let Some((params, _)) = find(&call.name) else {
            let message = format!("host action `{}` is not registered", call.name);
            return Err(RuntimeError::new(call.span, message).into());
        };
        if params != call.types {
            let message = format!(
                "host action `{}` takes {:?}, called with {:?}",
                call.name, params, call.types
            );
            return Err(RuntimeError::new(call.span, message).into());
        }
    }
    Ok(())
}

fn find(name: &str) -> Option<(Vec<Type>, Run)> {
    ACTIONS.with_borrow(|actions| {
        let action = actions.get(name)?;
        Some((action.params.clone(), action.run.clone()))
    })
}

/// Runs an action; an action must not call itself back through the engine.
pub(crate) fn call(name: &str, arguments: &[Data], span: Span) -> Result {
    let (_, run) = find(name).expect("validated when the program was built");
    let mut run = run.try_borrow_mut().map_err(|_| {
        RuntimeError::new(
            span,
            format!("host action `{name}` was called re-entrantly"),
        )
    })?;
    run(arguments)
}
