//! Host actions and the limits the engine enforces on a program.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use aegle_markup::{Span, Type};
use aegle_ui::Result;

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
    /// Actions for programs that lack their own, such as those `ui!` builds.
    static SHARED: Actions = Actions::default();
}

/// Registers an action for every program on this thread that has none of that
/// name, which is how `ui!` views reach the host. See [`crate::Program::action`].
pub fn register_shared(name: &str, params: &[Type], run: impl FnMut(&[Data]) -> Result + 'static) {
    SHARED.with(|shared| shared.register(name, params, run));
}

/// Actions registered by the host, shared by every clone of a program.
#[derive(Default)]
pub(crate) struct Actions(RefCell<HashMap<String, Action>>);

impl Actions {
    pub fn register(
        &self,
        name: &str,
        params: &[Type],
        run: impl FnMut(&[Data]) -> Result + 'static,
    ) {
        self.0.borrow_mut().insert(
            name.to_owned(),
            Action {
                params: params.to_vec(),
                run: Rc::new(RefCell::new(run)),
            },
        );
    }

    /// Copies registrations this registry lacks, so a reload keeps its actions.
    pub fn inherit(&self, from: &Actions) {
        let mut mine = self.0.borrow_mut();
        for (name, action) in from.0.borrow().iter() {
            mine.entry(name.clone()).or_insert_with(|| Action {
                params: action.params.clone(),
                run: action.run.clone(),
            });
        }
    }

    /// Checks every `host.name(...)` call of `program` against the registry.
    pub fn validate(&self, program: &aegle_markup::Program) -> Result {
        for call in &program.host_calls {
            let Some((params, _)) = self.find(&call.name) else {
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

    /// This registry's action, else the thread's shared one.
    fn find(&self, name: &str) -> Option<(Vec<Type>, Run)> {
        let own = |actions: &Actions| {
            actions
                .0
                .borrow()
                .get(name)
                .map(|action| (action.params.clone(), action.run.clone()))
        };
        own(self).or_else(|| SHARED.with(own))
    }

    /// Runs an action; an action must not call itself back through the engine.
    pub fn call(&self, name: &str, arguments: &[Data], span: Span) -> Result {
        let (_, run) = self
            .find(name)
            .expect("validated when the program was built");
        let mut run = run.try_borrow_mut().map_err(|_| {
            RuntimeError::new(
                span,
                format!("host action `{name}` was called re-entrantly"),
            )
        })?;
        run(arguments)
    }
}
