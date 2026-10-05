//! State cells and effects. An effect re-runs when a cell it read changes.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use aegle_app::Result;

use crate::Data;

type Run = Box<dyn FnMut(&Rc<Effect>) -> Result>;

/// One typed state value and the effects that read it.
pub(crate) struct Cell {
    value: RefCell<Data>,
    subscribers: RefCell<Vec<Weak<Effect>>>,
}

impl Cell {
    pub fn new(value: Data) -> Rc<Self> {
        Rc::new(Self {
            value: RefCell::new(value),
            subscribers: RefCell::new(Vec::new()),
        })
    }

    /// Sets the initial value before any effect reads it.
    pub fn init(&self, value: Data) {
        *self.value.borrow_mut() = value;
    }

    /// Reads the value, subscribing `effect` to later changes.
    pub fn get(&self, effect: Option<&Rc<Effect>>) -> Data {
        if let Some(effect) = effect {
            effect.depend(self);
        }
        self.value.borrow().clone()
    }

    /// Stores a changed value and re-runs live subscribers in subscription order.
    /// The first failing effect stops this update and returns its error.
    pub fn set(&self, value: Data) -> Result {
        if *self.value.borrow() == value {
            return Ok(());
        }
        *self.value.borrow_mut() = value;
        let subscribers = {
            let mut subscribers = self.subscribers.borrow_mut();
            subscribers.retain(|effect| effect.strong_count() > 0);
            subscribers.clone()
        };
        for effect in subscribers.iter().filter_map(Weak::upgrade) {
            effect.run()?;
        }
        Ok(())
    }
}

/// A binding, block or other reaction owned by the interface it updates.
pub(crate) struct Effect {
    run: RefCell<Option<Run>>,
    /// Addresses of the cells already subscribed to. Every cell an effect can
    /// read is kept alive by the environment the effect owns, so an address
    /// cannot be reused while the effect exists.
    cells: RefCell<Vec<*const Cell>>,
}

impl Effect {
    /// Creates an effect and runs it once to apply initial values and subscribe.
    pub fn new(run: impl FnMut(&Rc<Effect>) -> Result + 'static) -> Result<Rc<Self>> {
        let effect = Rc::new(Self {
            run: RefCell::new(Some(Box::new(run))),
            cells: RefCell::new(Vec::new()),
        });
        effect.run()?;
        Ok(effect)
    }

    /// Runs the effect unless it is already running.
    pub fn run(self: &Rc<Self>) -> Result {
        let Some(mut run) = self.run.borrow_mut().take() else {
            return Ok(());
        };
        let result = run(self);
        *self.run.borrow_mut() = Some(run);
        result
    }

    fn depend(self: &Rc<Self>, cell: &Cell) {
        let address = cell as *const Cell;
        if self.cells.borrow().contains(&address) {
            return;
        }
        self.cells.borrow_mut().push(address);
        let mut subscribers = cell.subscribers.borrow_mut();
        // Prune dropped effects before growing, keeping the list bounded.
        if subscribers.len() == subscribers.capacity() {
            subscribers.retain(|effect| effect.strong_count() > 0);
        }
        subscribers.push(Rc::downgrade(self));
    }
}
