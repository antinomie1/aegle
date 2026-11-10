//! Reactive values for applications written in Rust.
//!
//! A [`Signal`] holds one value on the UI thread, and a [`Derived`] value is
//! computed from signals. [`Signal::bind`] ties a reaction to a control: it
//! runs at once, and again whenever a signal it read changes, for as long as
//! the control lives. Dependencies are the signals the reaction read on its
//! latest run, so a branch not taken does not subscribe.
//!
//! A change queues the reactions that read the value. They run when the
//! change ends, before [`Signal::set`] returns, or, inside a [`batch`], when
//! the outermost batch ends. A pass runs derived values first, then bindings,
//! then change handlers, so a binding never sees a derived value that is out
//! of date; a reaction queued again before it runs still runs once. Changes
//! made by a running reaction join the pass in progress. Reactions only
//! change controls through their handles, which mark the tree dirty, so
//! layout and painting still happen once, at the next refresh.

use std::{
    any::Any,
    cell::{Cell, OnceCell, RefCell},
    collections::VecDeque,
    fmt,
    rc::{Rc, Weak},
};

use crate::{Node, Subscription};

thread_local! {
    /// The reaction whose reads subscribe it, while one runs.
    static CURRENT: RefCell<Option<Rc<Effect>>> = const { RefCell::new(None) };
    /// Reactions waiting for the current pass.
    static QUEUE: Queue = Queue::default();
}

/// Runs `change`, deferring the reactions to the signals it changes until it
/// returns, so each reaction runs once with the final values.
///
/// Batches nest: the reactions run when the outermost one ends. Until then,
/// bindings and derived values keep showing the values from before the batch;
/// the changed signals themselves read their new values. Event handlers
/// usually need no batch: a change there already runs its reactions once,
/// and layout and painting wait for the next refresh anyway.
///
/// ```
/// use aegle_ui::{Signal, batch};
///
/// let (first, last) = (Signal::new("Ada"), Signal::new("Lovelace"));
/// let (a, b) = (first.clone(), last.clone());
/// let full = first.map(move |first| format!("{first} {}", b.get()));
/// let changes = std::rc::Rc::new(std::cell::Cell::new(0));
/// let count = changes.clone();
/// full.on_change(move |_| count.set(count.get() + 1));
/// batch(|| {
///     a.set("Grace");
///     last.set("Hopper");
///     assert_eq!(full.get(), "Ada Lovelace"); // Not yet recomputed.
/// });
/// assert_eq!((full.get(), changes.get()), ("Grace Hopper".into(), 1));
/// ```
pub fn batch<R>(change: impl FnOnce() -> R) -> R {
    let result = {
        let _batch = Pass::enter();
        change()
    };
    flush();
    result
}

/// What runs a reaction, in the order a pass runs them.
#[derive(Clone, Copy)]
enum Kind {
    /// Recomputes a derived value.
    Derived,
    /// A binding to a control; it reads the derived values.
    Binding,
    /// A change handler, run with the settled values; reads do not subscribe.
    Watcher,
}

#[derive(Default)]
struct Queue {
    /// Open batches and the pass in progress; reactions wait while nonzero.
    depth: Cell<u32>,
    /// Pending reactions by [`Kind`]. The deques keep their capacity, so a
    /// steady stream of changes does not allocate.
    pending: RefCell<[VecDeque<Rc<Effect>>; 3]>,
}

/// Queues `effect` for the current pass, unless it is already queued,
/// removed, or running: a reaction is not rerun by its own changes.
fn schedule(effect: Rc<Effect>) {
    if effect.queued.get() || effect.removed.get() || effect.react.borrow().is_none() {
        return;
    }
    effect.queued.set(true);
    let kind = effect.kind as usize;
    QUEUE.with(|queue| queue.pending.borrow_mut()[kind].push_back(effect));
}

/// Runs the queued reactions, earlier kinds first, unless a batch or pass is
/// open; reactions queued meanwhile join this pass.
fn flush() {
    if QUEUE.with(|queue| queue.depth.get()) > 0 {
        return;
    }
    let _pass = Pass::enter();
    while let Some(effect) = QUEUE.with(|queue| {
        let mut pending = queue.pending.borrow_mut();
        pending.iter_mut().find_map(VecDeque::pop_front)
    }) {
        effect.queued.set(false);
        effect.run();
    }
}

/// An open batch or pass. Closing it also when a reaction panics keeps later
/// changes running their reactions; the ones still queued then run in the
/// next pass.
struct Pass;

impl Pass {
    fn enter() -> Self {
        QUEUE.with(|queue| queue.depth.set(queue.depth.get() + 1));
        Self
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        QUEUE.with(|queue| queue.depth.set(queue.depth.get() - 1));
    }
}

/// A value on the UI thread that reactions follow.
///
/// Cloning shares the value. Reading it with [`Self::get`] or [`Self::with`]
/// inside a reaction subscribes the reaction. Changing it with [`Self::set`]
/// or [`Self::update`] reruns those reactions before `set` returns: derived
/// values first, so that bindings see them up to date, then bindings, then
/// change handlers. Inside a [`batch`], or inside a running reaction, they
/// wait until it ends.
///
/// A signal is not `Send`: a background thread reaches it through the
/// application's proxy, whose handler runs on the UI thread.
///
/// ```
/// use aegle_ui::Signal;
///
/// let count = Signal::new(1);
/// let doubled = Signal::new(0);
/// let target = doubled.clone();
/// let watch = count.on_change(move |n| target.set(n * 2));
/// count.set(4);
/// assert_eq!(doubled.get(), 8);
/// watch.remove();
/// count.set(5);
/// assert_eq!(doubled.get(), 8);
/// ```
pub struct Signal<T> {
    inner: Rc<Inner<T>>,
}

struct Inner<T> {
    value: RefCell<T>,
    /// Reactions that read the value, with the run that read it.
    readers: RefCell<Vec<(Weak<Effect>, u64)>>,
    /// Change handlers, owned by the signal and run after the readers.
    watchers: RefCell<Vec<Rc<Effect>>>,
    /// The reaction computing a derived value, owned by its signal.
    _compute: OnceCell<Rc<Effect>>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: Default + 'static> Default for Signal<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: fmt::Debug> fmt::Debug for Signal<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.value.try_borrow() {
            Ok(value) => f.debug_tuple("Signal").field(&*value).finish(),
            Err(_) => f.write_str("Signal(<being changed>)"),
        }
    }
}

impl<T: 'static> Signal<T> {
    /// A signal holding `value`, with no readers yet.
    pub fn new(value: T) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(value),
                readers: RefCell::new(Vec::new()),
                watchers: RefCell::new(Vec::new()),
                _compute: OnceCell::new(),
            }),
        }
    }

    /// Calls `read` with the value; inside a reaction, subscribes it.
    ///
    /// # Panics
    ///
    /// When `read` changes this signal.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.track();
        read(&self.inner.value.borrow())
    }

    /// Changes the value in place and reruns its reactions, even when `change`
    /// left it equal; [`Self::set`] skips equal values. Inside a [`batch`] or
    /// a running reaction, the reactions wait for its end.
    ///
    /// # Panics
    ///
    /// When called from a reading closure of this signal ([`Self::with`],
    /// [`Self::bind`]), and, through the reactions, wherever handle methods
    /// panic: from a painter, hook or scene visitor.
    pub fn update(&self, change: impl FnOnce(&mut T)) {
        change(&mut self.inner.value.borrow_mut());
        self.notify();
    }

    /// Runs `react` with the control's handle and this value now, then again
    /// whenever this signal or another signal it read changes, until the
    /// control is removed or its window closes, or [`Binding::remove`].
    ///
    /// `react` usually sets a property: `count.bind(&label, |label, n|
    /// label.set_text(&n.to_string()))`. It is not rerun by changes it makes
    /// itself, and must not change this signal, which it is reading; other
    /// signals it changes update their readers after it returns.
    ///
    /// # Panics
    ///
    /// When `control` was removed, and wherever handle methods panic.
    pub fn bind<H: AsRef<Node> + Clone + 'static>(
        &self,
        control: &H,
        mut react: impl FnMut(&H, &T) + 'static,
    ) -> Binding {
        let signal = self.clone();
        let handle = control.clone();
        let effect = Effect::new(Kind::Binding, move || {
            // A removed control's binding is dropped with it; until then, skip.
            if handle.as_ref().is_alive() {
                signal.with(|value| react(&handle, value));
            }
        });
        control.as_ref().keep_alive(effect.clone());
        batch(|| effect.run());
        Binding {
            effect: Rc::downgrade(&effect),
            subscription: None,
        }
    }

    /// Subscribes the running reaction, if any, to this signal.
    fn track(&self) {
        let Some(effect) = CURRENT.with_borrow(Clone::clone) else {
            return;
        };
        let source: Weak<dyn Any> = Rc::downgrade(&self.inner) as Weak<dyn Any>;
        let mut sources = effect.sources.borrow_mut();
        if sources.iter().any(|known| known.ptr_eq(&source)) {
            return;
        }
        sources.push(source);
        let mut readers = self.inner.readers.borrow_mut();
        // Prune dropped and outdated readers before growing, keeping the list
        // bounded by the live ones.
        if readers.len() == readers.capacity() {
            readers.retain(|(reader, run)| current(reader, *run).is_some());
        }
        readers.push((Rc::downgrade(&effect), effect.run.get()));
    }

    /// A value computed from this one, recomputed when it changes; see
    /// [`Derived::new`].
    pub fn map<U: PartialEq + 'static>(&self, map: impl Fn(&T) -> U + 'static) -> Derived<U> {
        let signal = self.clone();
        Derived::new(move || signal.with(&map))
    }

    /// Queues the reactions that read the value on their latest run, in the
    /// order they first read it, and the change handlers, then runs them
    /// unless a batch or pass is open.
    fn notify(&self) {
        self.inner.readers.borrow_mut().retain(|(reader, run)| {
            let Some(reader) = current(reader, *run) else {
                return false;
            };
            schedule(reader);
            true
        });
        self.inner.watchers.borrow_mut().retain(|watcher| {
            let live = !watcher.removed.get();
            if live {
                schedule(watcher.clone());
            }
            live
        });
        flush();
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// A copy of the value; inside a reaction, subscribes it.
    pub fn get(&self) -> T {
        self.with(T::clone)
    }

    /// Runs `handler` with the new value after every change, once the
    /// reactions that read it have rerun, until [`Binding::remove`]. Within a
    /// [`batch`] it runs once, with the final value.
    ///
    /// The signal owns the handler, so one that captures this signal keeps
    /// it alive until removed. Reads inside the handler subscribe nothing,
    /// and changes it makes to this signal do not rerun it.
    pub fn on_change(&self, mut handler: impl FnMut(T) + 'static) -> Binding {
        let signal = Rc::downgrade(&self.inner);
        let watcher = Effect::new(Kind::Watcher, move || {
            if let Some(signal) = signal.upgrade() {
                let value = signal.value.borrow().clone();
                handler(value);
            }
        });
        let binding = Binding {
            effect: Rc::downgrade(&watcher),
            subscription: None,
        };
        self.inner.watchers.borrow_mut().push(watcher);
        binding
    }
}

impl<T: PartialEq + 'static> Signal<T> {
    /// Replaces the value and reruns its reactions, unless it is equal to the
    /// current one.
    ///
    /// # Panics
    ///
    /// Like [`Self::update`].
    pub fn set(&self, value: T) {
        {
            let mut current = self.inner.value.borrow_mut();
            if *current == value {
                return;
            }
            *current = value;
        }
        self.notify();
    }
}

/// A read-only value computed from signals and other derived values.
///
/// The computation runs once at creation and again in the pass after a value
/// it read changes, before the bindings, so a binding reading it and its
/// sources sees them agree. Its readers rerun only when the result differs
/// from the previous one. Cloning shares the value; the computation lives as
/// long as a clone, or a binding reading it.
///
/// ```
/// use aegle_ui::{Derived, Signal};
///
/// let items = Signal::new(vec![3, 4]);
/// let source = items.clone();
/// let total = Derived::new(move || source.with(|items| items.iter().sum::<i32>()));
/// let even = total.map(|total| total % 2 == 0);
/// items.update(|items| items.push(5));
/// assert_eq!((total.get(), even.get()), (12, true));
/// ```
pub struct Derived<T>(Signal<T>);

impl<T> Clone for Derived<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: fmt::Debug> fmt::Debug for Derived<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.inner.value.try_borrow() {
            Ok(value) => f.debug_tuple("Derived").field(&*value).finish(),
            Err(_) => f.write_str("Derived(<being changed>)"),
        }
    }
}

impl<T: PartialEq + 'static> Derived<T> {
    /// Computes the value now, subscribing to the signals `compute` reads.
    pub fn new(mut compute: impl FnMut() -> T + 'static) -> Self {
        let target: Rc<OnceCell<Weak<Inner<T>>>> = Rc::default();
        let first: Rc<Cell<Option<T>>> = Rc::default();
        let (cell, initial) = (target.clone(), first.clone());
        let effect = Effect::new(Kind::Derived, move || {
            let value = compute();
            match cell.get().and_then(Weak::upgrade) {
                Some(inner) => Signal { inner }.set(value),
                None => initial.set(Some(value)),
            }
        });
        batch(|| effect.run());
        let signal = Signal::new(first.take().expect("the first run computes the value"));
        let _ = target.set(Rc::downgrade(&signal.inner));
        let _ = signal.inner._compute.set(effect);
        Self(signal)
    }
}

impl<T: 'static> Derived<T> {
    /// Calls `read` with the value; inside a reaction, subscribes it.
    pub fn with<R>(&self, read: impl FnOnce(&T) -> R) -> R {
        self.0.with(read)
    }

    /// Like [`Signal::bind`]: runs `react` now and whenever the value
    /// changes, while `control` lives.
    pub fn bind<H: AsRef<Node> + Clone + 'static>(
        &self,
        control: &H,
        react: impl FnMut(&H, &T) + 'static,
    ) -> Binding {
        self.0.bind(control, react)
    }

    /// A value computed from this one; see [`Signal::map`].
    pub fn map<U: PartialEq + 'static>(&self, map: impl Fn(&T) -> U + 'static) -> Derived<U> {
        self.0.map(map)
    }
}

impl<T: Clone + 'static> Derived<T> {
    /// A copy of the value; inside a reaction, subscribes it.
    pub fn get(&self) -> T {
        self.0.get()
    }

    /// Like [`Signal::on_change`]: runs `handler` after each change of the
    /// value.
    pub fn on_change(&self, handler: impl FnMut(T) + 'static) -> Binding {
        self.0.on_change(handler)
    }
}

/// The reaction `reader` if it still holds the dependencies of run `run`.
fn current(reader: &Weak<Effect>, run: u64) -> Option<Rc<Effect>> {
    reader
        .upgrade()
        .filter(|effect| effect.run.get() == run && !effect.removed.get())
}

/// A reaction registered by [`Signal::bind`] or [`Signal::on_change`] (or
/// their [`Derived`] counterparts).
///
/// Dropping it keeps the reaction; it ends with its control or signal.
#[derive(Clone)]
pub struct Binding {
    effect: Weak<Effect>,
    subscription: Option<Subscription>,
}

impl fmt::Debug for Binding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Binding")
            .field("active", &self.is_active())
            .finish()
    }
}

impl Binding {
    /// Stops the reaction: it never runs again, and its closure is dropped
    /// unless it is running. Removing it twice, or after its control or
    /// signal is gone, does nothing.
    pub fn remove(&self) {
        if let Some(effect) = self.effect.upgrade() {
            effect.removed.set(true);
            // Taking the closure out of the cell first lets its drop reach
            // signals and handles without this borrow held.
            let closure = effect.react.borrow_mut().take();
            drop(closure);
        }
        if let Some(subscription) = &self.subscription {
            subscription.remove();
        }
    }

    /// Whether the reaction can still run.
    pub fn is_active(&self) -> bool {
        self.effect
            .upgrade()
            .is_some_and(|effect| !effect.removed.get())
    }

    /// Ties a handler subscription to this binding, so that
    /// [`Self::remove`] removes both; for two-way bindings of controls, whose
    /// change handler writes the signal back.
    pub fn with_subscription(mut self, subscription: Subscription) -> Self {
        self.subscription = Some(subscription);
        self
    }
}

/// A reaction: a closure rerun when a signal it read changes.
struct Effect {
    /// The closure; taken out while it runs.
    react: RefCell<Option<Box<dyn FnMut()>>>,
    kind: Kind,
    /// Whether it waits in the queue.
    queued: Cell<bool>,
    /// Counts runs; subscriptions of an earlier run are outdated.
    run: Cell<u64>,
    /// The signals read on the latest run, kept to subscribe once per signal.
    sources: RefCell<Vec<Weak<dyn Any>>>,
    removed: Cell<bool>,
}

impl Effect {
    fn new(kind: Kind, react: impl FnMut() + 'static) -> Rc<Self> {
        Rc::new(Self {
            react: RefCell::new(Some(Box::new(react))),
            kind,
            queued: Cell::new(false),
            run: Cell::new(0),
            sources: RefCell::new(Vec::new()),
            removed: Cell::new(false),
        })
    }

    /// Runs the reaction unless it is already running or removed, replacing
    /// its dependencies with the signals it reads now.
    fn run(self: &Rc<Self>) {
        let Some(mut react) = self.react.borrow_mut().take() else {
            return;
        };
        self.run.set(self.run.get() + 1);
        self.sources.borrow_mut().clear();
        {
            let tracks = !matches!(self.kind, Kind::Watcher);
            let _tracking = Tracking::enter(tracks.then(|| self.clone()));
            react();
        }
        if !self.removed.get() {
            *self.react.borrow_mut() = Some(react);
        }
    }
}

/// Sets the running reaction and restores the previous one when dropped,
/// also when a reaction panics.
struct Tracking(Option<Rc<Effect>>);

impl Tracking {
    fn enter(effect: Option<Rc<Effect>>) -> Self {
        Self(CURRENT.replace(effect))
    }
}

impl Drop for Tracking {
    fn drop(&mut self) {
        CURRENT.set(self.0.take());
    }
}
