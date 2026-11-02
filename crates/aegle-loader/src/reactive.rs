//! State cells and effects. An effect re-runs when a cell it read changes.

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use aegle_ui::Result;

use crate::Data;

type Run = Box<dyn FnMut(&Rc<Effect>) -> Result>;

/// A host handler of a state's changes, by registration number.
pub(crate) type Watcher = (u64, Rc<RefCell<dyn FnMut(&Data) -> Result>>);

/// One typed state value, the effects that read it and the host's watchers.
pub(crate) struct Cell {
    value: RefCell<Data>,
    subscribers: RefCell<Vec<Weak<Effect>>>,
    watchers: RefCell<Vec<Watcher>>,
}

impl Cell {
    pub fn new(value: Data) -> Rc<Self> {
        Rc::new(Self {
            value: RefCell::new(value),
            subscribers: RefCell::new(Vec::new()),
            watchers: RefCell::new(Vec::new()),
        })
    }

    /// Adds a host watcher, run after the effects on every change.
    pub fn watch(&self, watcher: Watcher) {
        self.watchers.borrow_mut().push(watcher);
    }

    /// Removes the watcher registered as `id`, if it is still here.
    pub fn unwatch(&self, id: u64) {
        self.watchers.borrow_mut().retain(|(n, _)| *n != id);
    }

    /// Takes over the watchers of the cell this one replaces in a reload.
    pub fn adopt(&self, old: &Cell) {
        let moved = std::mem::take(&mut *old.watchers.borrow_mut());
        self.watchers.borrow_mut().extend(moved);
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

    /// Stores a changed value and re-runs live subscribers in subscription
    /// order, then the watchers in registration order with the new value.
    /// The first failure stops this update and returns its error.
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
        if self.watchers.borrow().is_empty() {
            return Ok(());
        }
        let watchers = self.watchers.borrow().clone();
        for (id, watcher) in watchers {
            // A watcher removed by an earlier one no longer runs.
            if !self.watchers.borrow().iter().any(|(n, _)| *n == id) {
                continue;
            }
            // Bindings settle synchronously, so a watcher sees the latest value.
            let value = self.value.borrow().clone();
            // Like an effect, a watcher is not re-entered by its own changes.
            let Ok(mut watcher) = watcher.try_borrow_mut() else {
                continue;
            };
            watcher(&value)?;
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
