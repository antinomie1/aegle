//! Reactive values for applications written in Rust.
//!
//! A [`Signal`] holds one value on the UI thread. [`Signal::bind`] ties a
//! reaction to a control: it runs once at once, and again whenever a signal it
//! read changes, for as long as the control lives. Dependencies are the signals
//! the reaction read on its latest run, so a branch not taken does not
//! subscribe. Reactions run synchronously inside [`Signal::set`], before it
//! returns; they only change controls through their handles, which mark the
//! tree dirty, so layout and painting still happen once, at the next refresh.

use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    rc::{Rc, Weak},
};

use crate::{Node, Subscription};

thread_local! {
    /// The reaction whose reads subscribe it, while one runs.
    static CURRENT: RefCell<Option<Rc<Effect>>> = const { RefCell::new(None) };
}

/// A value on the UI thread that reactions follow.
///
/// Cloning shares the value. Reading it with [`Self::get`] or [`Self::with`]
/// inside a reaction subscribes the reaction; changing it with [`Self::set`]
/// or [`Self::update`] reruns those reactions, then the change handlers. A
/// signal is not `Send`: a background thread reaches it through the
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
    /// left it equal; [`Self::set`] skips equal values.
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
    /// itself, and must not change this signal, which it is reading.
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
        let effect = Effect::new(true, move || {
            // A removed control's binding is dropped with it; until then, skip.
            if handle.as_ref().is_alive() {
                signal.with(|value| react(&handle, value));
            }
        });
        control.as_ref().keep_alive(effect.clone());
        effect.run();
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

    /// Reruns the reactions that read the value on their latest run, in the
    /// order they first read it, then the change handlers.
    fn notify(&self) {
        let readers: Vec<_> = {
            let mut readers = self.inner.readers.borrow_mut();
            readers.retain(|(reader, run)| current(reader, *run).is_some());
            readers.clone()
        };
        let _untracked = Tracking::enter(None);
        for (reader, run) in readers {
            // A reaction that ran meanwhile and stopped reading is skipped.
            if let Some(effect) = current(&reader, run) {
                effect.run();
            }
        }
        let watchers = {
            let mut watchers = self.inner.watchers.borrow_mut();
            watchers.retain(|watcher| !watcher.removed.get());
            if watchers.is_empty() {
                return;
            }
            watchers.clone()
        };
        for watcher in watchers {
            watcher.run();
        }
    }
}

impl<T: Clone + 'static> Signal<T> {
    /// A copy of the value; inside a reaction, subscribes it.
    pub fn get(&self) -> T {
        self.with(T::clone)
    }

    /// Runs `handler` with the new value after every change, once the
    /// reactions that read it have rerun, until [`Binding::remove`].
    ///
    /// The signal owns the handler, so one that captures this signal keeps
    /// it alive until removed. Reads inside the handler subscribe nothing,
    /// and changes it makes to this signal do not rerun it.
    pub fn on_change(&self, mut handler: impl FnMut(T) + 'static) -> Binding {
        let signal = Rc::downgrade(&self.inner);
        let watcher = Effect::new(false, move || {
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

/// The reaction `reader` if it still holds the dependencies of run `run`.
fn current(reader: &Weak<Effect>, run: u64) -> Option<Rc<Effect>> {
    reader
        .upgrade()
        .filter(|effect| effect.run.get() == run && !effect.removed.get())
}

/// A reaction registered by [`Signal::bind`] or [`Signal::on_change`].
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
    react: RefCell<Option<Box<dyn FnMut()>>>,
    /// Whether reads during a run subscribe this reaction.
    tracks: bool,
    /// Counts runs; subscriptions of an earlier run are outdated.
    run: Cell<u64>,
    /// The signals read on the latest run, kept to subscribe once per signal.
    sources: RefCell<Vec<Weak<dyn Any>>>,
    removed: Cell<bool>,
}

impl Effect {
    fn new(tracks: bool, react: impl FnMut() + 'static) -> Rc<Self> {
        Rc::new(Self {
            react: RefCell::new(Some(Box::new(react))),
            tracks,
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
            let _tracking = Tracking::enter(self.tracks.then(|| self.clone()));
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
