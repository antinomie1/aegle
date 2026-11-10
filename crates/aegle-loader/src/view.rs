//! Built interfaces: root, named controls, entry states and reloading.

use std::{
    any::Any,
    cell::RefCell,
    marker::PhantomData,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};

use aegle_markup::{ElementKind, Span, Type};
use aegle_ui::{Binding, Container, Derived, HandlerResult, Node, Result, Signal};

use crate::{
    Data, Program, RuntimeError,
    build::{self, Block, Carried, Ids},
    eval::Env,
    handle::Handle,
    reactive::Cell,
};

/// A built markup interface: its root, named controls and entry-document states.
///
/// Bindings and blocks live as long as the root control, even if the view is
/// dropped; the view only offers access. An event block occupies its control's
/// handler slot, so replacing that handler from Rust disables the block.
pub struct View {
    program: Program,
    root: Handle,
    env: Env,
    ids: Ids,
    /// Effects of the entry document, also kept alive by the root control.
    effects: Rc<RefCell<Block>>,
    /// Top-level controls of a window's content, replaced by a reload.
    content: Vec<Handle>,
    /// Container of a fragment root.
    parent: Option<Container>,
    /// States that typed handles share, repointed by a reload.
    slots: RefCell<Vec<Rc<Slot>>>,
    /// A Window document's role and output, which a reload keeps.
    #[cfg(feature = "window")]
    surface: Option<crate::window::Surface>,
}

/// The state values of an interface that a reload keeps: entry states, and
/// the states of instances named by the same id and component, by name and
/// type.
fn carry<'a>(
    program: &'a Program,
    env: &'a Env,
    ids: &'a Ids,
) -> impl Fn(Option<(&str, &str)>, &str, &Type) -> Option<Data> + 'a {
    move |owner, name, ty| {
        let checked = &program.0.checked;
        let (template, env) = match owner {
            None => (0, env),
            Some((id, component)) => {
                let index = checked.ids.iter().position(|(n, _)| n == id)?;
                let template = instance_template(checked, index)?;
                if checked.templates[template].name != component {
                    return None;
                }
                (template, ids.instances[index].as_ref()?)
            }
        };
        let states = &checked.templates[template].states;
        let index = states.iter().position(|(n, t, _)| n == name && t == ty)?;
        Some(env.scope.states[index].get(None))
    }
}

/// The component that entry id `index` names, if it names an instance.
fn instance_template(program: &aegle_markup::Program, index: usize) -> Option<usize> {
    match program.ids[index].1 {
        ElementKind::Component(template) => Some(template),
        _ => None,
    }
}

pub(crate) fn fragment(program: &Program, parent: &Container, carried: Carried) -> Result<View> {
    let root = &program.0.checked.templates[0].root;
    if root.kind == ElementKind::Window {
        return Err("a Window document is opened with Program::open".into());
    }
    crate::actions::validate(&program.0.checked)?;
    let entry = |name: &str, ty: &Type| carried(None, name, ty);
    let env = Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), &entry)?;
    let mut ids = Ids::new(program.0.checked.ids.len());
    let (mut block, mut created) = (Block::new(), Vec::new());
    let result = build::root(
        root,
        parent,
        &env,
        &mut block,
        &mut created,
        &mut ids,
        carried,
    );
    if let Err(error) = result {
        if let Some(root) = created.first() {
            root.node().remove();
        }
        return Err(error);
    }
    let root = created.swap_remove(0);
    let _ = env.scope.named.set(ids.controls.as_slice().into());
    let effects = Rc::new(RefCell::new(block));
    root.node().keep_alive(effects.clone());
    Ok(View {
        program: program.clone(),
        root,
        env,
        ids,
        effects,
        content: Vec::new(),
        parent: Some(parent.clone()),
        slots: RefCell::new(Vec::new()),
        #[cfg(feature = "window")]
        surface: None,
    })
}

#[cfg(feature = "window")]
pub(crate) fn window(
    program: &Program,
    app: &aegle_app::App,
    output: Option<aegle_app::OutputId>,
) -> Result<View> {
    let root = &program.0.checked.templates[0].root;
    if root.kind != ElementKind::Window {
        return Err("Program::open requires a Window document root".into());
    }
    crate::actions::validate(&program.0.checked)?;
    let (window, surface) = crate::window::open(app, root, output)?;
    let handle = Handle::window(window.clone());
    let effects = Rc::new(RefCell::new(Block::new()));
    let mut view = View {
        program: program.clone(),
        root: handle.clone(),
        env: Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), &|_, _| None)
            .inspect_err(|_| drop(window.close()))?,
        ids: Ids::default(),
        effects: effects.clone(),
        content: Vec::new(),
        parent: None,
        slots: RefCell::new(Vec::new()),
        surface: Some(surface),
    };
    if let Err(error) = view.fill(program, view.env.clone(), &|_, _, _| None) {
        window.close()?;
        return Err(error);
    }
    handle.node().keep_alive(effects);
    Ok(view)
}

impl View {
    /// The root control; a Window document's root is its window.
    pub fn root(&self) -> &Handle {
        &self.root
    }

    /// The control named by `id` in the entry document.
    pub fn handle(&self, id: &str) -> Option<&Handle> {
        let index = self
            .program
            .0
            .checked
            .ids
            .iter()
            .position(|(name, _)| name == id)?;
        self.ids.controls[index].as_ref()
    }

    /// The control with entry ID number `index`, in declaration order; for an
    /// ID naming a component instance, the instance's root control.
    pub fn id(&self, index: usize) -> &Handle {
        self.ids.controls[index]
            .as_ref()
            .expect("entry IDs are built with the view")
    }

    /// The component instance named by `id` in the entry document.
    pub fn instance(&self, id: &str) -> Option<Instance<'_>> {
        let ids = &self.program.0.checked.ids;
        let index = ids.iter().position(|(name, _)| name == id)?;
        instance_template(&self.program.0.checked, index)?;
        Some(Instance { view: self, index })
    }

    /// The component instance with entry ID number `index`; the ID must name
    /// an instance.
    pub fn instance_at(&self, index: usize) -> Instance<'_> {
        debug_assert!(instance_template(&self.program.0.checked, index).is_some());
        Instance { view: self, index }
    }

    /// The current value of an entry-document state.
    pub fn get(&self, name: &str) -> Option<Data> {
        let (index, _) = self.state_index(name)?;
        Some(self.env.scope.states[index].get(None))
    }

    /// Assigns an entry-document state and updates every binding that reads it.
    /// The value must have the declared type; floats must be finite.
    pub fn set(&self, name: &str, value: Data) -> Result {
        let (index, ty) = self
            .state_index(name)
            .ok_or_else(|| RuntimeError::new(Span::default(), format!("unknown state `{name}`")))?;
        if !conforms(&value, ty, &self.program.0.checked) {
            let message = format!("state `{name}` requires {ty:?}");
            return Err(RuntimeError::new(Span::default(), message).into());
        }
        self.env.scope.states[index].set(value)
    }

    /// A typed handle to an entry-document state, if `T` matches its type.
    pub fn state<T: StateValue>(&self, name: &str) -> Option<State<T>> {
        let (index, ty) = self.state_index(name)?;
        T::accepts(ty).then(|| self.state_at(index))
    }

    /// A typed handle to entry state number `index`; `T` must match its type.
    pub fn state_at<T: StateValue>(&self, index: usize) -> State<T> {
        debug_assert!(T::accepts(
            &self.program.0.checked.templates[0].states[index].1
        ));
        State {
            slot: self.slot(None, index),
            marker: PhantomData,
        }
    }

    /// The slot shared by handles to state `index` of `owner`: the entry
    /// document, or the instance with that entry id.
    fn slot(&self, owner: Option<usize>, index: usize) -> Rc<Slot> {
        let mut slots = self.slots.borrow_mut();
        let same = |slot: &&Rc<Slot>| slot.owner.get() == owner && slot.index.get() == index;
        if let Some(slot) = slots.iter().find(same) {
            return slot.clone();
        }
        let env = match owner {
            None => &self.env,
            Some(id) => {
                (self.ids.instances[id].as_ref()).expect("named instances are built with the view")
            }
        };
        let slot = Rc::new(Slot {
            cell: RefCell::new(env.scope.states[index].clone()),
            program: RefCell::new(self.program.clone()),
            owner: std::cell::Cell::new(owner),
            index: std::cell::Cell::new(index),
            root: RefCell::new(self.root.node().clone()),
            followed: RefCell::new(Vec::new()),
        });
        slots.push(slot.clone());
        slot
    }

    /// Hands every typed state handle to the state of the same name and type
    /// in `next`, of the entry document or of the instance named by the same
    /// id and component, with its change handlers; others keep their
    /// detached state.
    fn hand_over(&self, next: &Program, env: &Env, ids: &Ids) -> Vec<Rc<Slot>> {
        let (old, new) = (&self.program.0.checked, &next.0.checked);
        let mut slots = Vec::new();
        for slot in self.slots.borrow().iter() {
            let template = template_of(old, slot.owner.get());
            let (name, ty, _) = &old.templates[template].states[slot.index.get()];
            let owner = match slot.owner.get() {
                None => None,
                Some(id) => {
                    let component = &old.templates[template].name;
                    let found = new.ids.iter().enumerate().position(|(index, (n, _))| {
                        *n == old.ids[id].0
                            && instance_template(new, index)
                                .is_some_and(|t| new.templates[t].name == *component)
                    });
                    match found {
                        Some(index) => Some(index),
                        None => continue,
                    }
                }
            };
            let (template, env) = match owner {
                None => (0, env),
                Some(id) => (
                    template_of(new, owner),
                    (ids.instances[id].as_ref()).expect("named instances are built with the view"),
                ),
            };
            let states = &new.templates[template].states;
            let Some(index) = states.iter().position(|(n, t, _)| n == name && t == ty) else {
                continue;
            };
            let cell = env.scope.states[index].clone();
            cell.adopt(&slot.cell.borrow());
            *slot.cell.borrow_mut() = cell;
            *slot.program.borrow_mut() = next.clone();
            slot.owner.set(owner);
            slot.index.set(index);
            slots.push(slot.clone());
        }
        slots
    }

    /// Replaces this interface with one built from `program`, atomically: the
    /// new interface is fully built before the old one is removed, and a
    /// failure keeps the old one. Entry states with the same name and type
    /// keep their values, and their typed [`State`] handles and change
    /// handlers follow them; other control-local state is reset. A Window
    /// document keeps its native window, title and size, and rebuilds its
    /// content; window properties the new version omits keep their values.
    /// A layer window keeps its surface and output and applies the new
    /// layer, anchor, zone, keyboard policy, margin and size in place; a
    /// reload that adds or removes `layer` or names another output fails.
    pub fn reload(&mut self, program: &Program) -> Result {
        let window = self.program.0.checked.templates[0].root.kind == ElementKind::Window;
        let fresh = program.0.checked.templates[0].root.kind == ElementKind::Window;
        if window != fresh {
            return Err("a reload cannot change whether the root is a Window".into());
        }
        if let Some(parent) = &self.parent {
            let carried = carry(&self.program, &self.env, &self.ids);
            let view = fragment(program, parent, &carried);
            drop(carried);
            let view = view?;
            let slots = self.hand_over(program, &view.env, &view.ids);
            for slot in &slots {
                slot.anchor(view.root.node());
            }
            *view.slots.borrow_mut() = slots;
            self.root.node().remove();
            *self = view;
            return Ok(());
        }
        crate::actions::validate(&program.0.checked)?;
        #[cfg(feature = "window")]
        let layer = match &self.surface {
            Some(surface) => surface.reload(&program.0.checked.templates[0].root)?,
            None => None,
        };
        let (previous, old_env) = (self.program.clone(), self.env.clone());
        let old = std::mem::take(&mut self.content);
        let (old_ids, old_block) = (std::mem::take(&mut self.ids), self.effects.take());
        let carried = carry(&previous, &old_env, &old_ids);
        let entry = |name: &str, ty: &Type| carried(None, name, ty);
        let built = Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), &entry)
            .and_then(|env| self.fill(program, env, &carried));
        drop(carried);
        if let Err(error) = built {
            for handle in std::mem::replace(&mut self.content, old) {
                handle.node().remove();
            }
            self.ids = old_ids;
            *self.effects.borrow_mut() = old_block;
            return Err(error);
        }
        for handle in old {
            handle.node().remove();
        }
        let current = std::mem::replace(&mut self.program, previous);
        let slots = self.hand_over(&current, &self.env, &self.ids);
        self.program = current;
        *self.slots.borrow_mut() = slots;
        // The surface stays; only its placement changes, in place.
        #[cfg(feature = "window")]
        if let Some((layer, width, height)) = layer {
            let window = self.root.typed::<aegle_app::Window>().unwrap();
            window.set_layer(layer, width, height)?;
        }
        Ok(())
    }

    /// Builds a Window document's content and window properties into `self`.
    fn fill(&mut self, program: &Program, env: Env, carried: Carried) -> Result {
        let root = &program.0.checked.templates[0].root;
        let mut ids = Ids::new(program.0.checked.ids.len());
        if let Some(index) = root.id {
            ids.controls[index] = Some(self.root.clone());
        }
        #[cfg(feature = "window")]
        for (prop, bound) in &root.properties {
            use aegle_markup::{Bound, Prop, PropertyName, Value};
            if let (Prop::Node(PropertyName::Theme), Bound::Literal(Value::Identifier(theme))) =
                (prop, bound)
            {
                let window = self.root.typed::<aegle_app::Window>().unwrap();
                window.set_theme(match theme.as_str() {
                    "light" => aegle_ui::Theme::light(),
                    "dark" => aegle_ui::Theme::dark(),
                    "high_contrast" => aegle_ui::Theme::high_contrast(),
                    other => unreachable!("checked theme `{other}`"),
                });
            }
        }
        let mut block = Block::new();
        build::window(
            root,
            &self.root,
            &env,
            &mut block,
            &mut self.content,
            &mut ids,
            carried,
        )?;
        *self.effects.borrow_mut() = block;
        let _ = env.scope.named.set(ids.controls.as_slice().into());
        self.program = program.clone();
        self.env = env;
        self.ids = ids;
        Ok(())
    }

    fn state_index(&self, name: &str) -> Option<(usize, &Type)> {
        let states = &self.program.0.checked.templates[0].states;
        let index = states.iter().position(|(n, _, _)| n == name)?;
        Some((index, &states[index].1))
    }
}

fn conforms(value: &Data, ty: &Type, program: &aegle_markup::Program) -> bool {
    match (value, ty) {
        (Data::Bool(_), Type::Bool)
        | (Data::Int(_), Type::Int)
        | (Data::String(_), Type::String) => true,
        (Data::Float(value), Type::Float) => value.is_finite(),
        (Data::List(items), Type::List(item)) => {
            items.iter().all(|value| conforms(value, item, program))
        }
        (Data::Record(values), Type::Record(name)) => {
            let record = program.records.iter().find(|record| record.name == *name);
            record.is_some_and(|record| {
                record.fields.len() == values.len()
                    && record
                        .fields
                        .iter()
                        .zip(values.iter())
                        .all(|((_, ty), value)| conforms(value, ty, program))
            })
        }
        _ => false,
    }
}

/// One state as its typed handles see it. A reload repoints it to the new
/// interface's state of the same name and type, and owner.
pub(crate) struct Slot {
    cell: RefCell<Rc<Cell>>,
    /// The program whose records a value is checked against.
    program: RefCell<Program>,
    /// The entry id of the instance owning the state; `None` for the entry
    /// document.
    owner: std::cell::Cell<Option<usize>>,
    /// The state's index in its owner's template.
    index: std::cell::Cell<usize>,
    /// The interface's root control, which keeps what the state follows.
    root: RefCell<Node>,
    /// What the state follows ([`State::follow`]), to move to a new root.
    followed: RefCell<Vec<Weak<dyn Any>>>,
}

/// The template declaring the states of `owner`.
fn template_of(program: &aegle_markup::Program, owner: Option<usize>) -> usize {
    owner.map_or(0, |id| {
        instance_template(program, id).expect("instance ids name components")
    })
}

impl Slot {
    /// Moves what the state follows to `root`, which replaces the old one.
    fn anchor(&self, root: &Node) {
        let mut followed = self.followed.borrow_mut();
        followed.retain(|held| held.strong_count() > 0);
        for held in followed.iter().filter_map(Weak::upgrade) {
            root.keep_alive(held);
        }
        *self.root.borrow_mut() = root.clone();
    }
}

/// A value a state follows and the state, kept by the interface's root
/// until the link is removed.
type Followed<T> = RefCell<Option<(Derived<T>, State<T>)>>;

/// A component instance named by an `id` where the entry document uses the
/// component, outside blocks: its root control and typed handles to its
/// states. A reload keeps an instance's state values, and hands its handles
/// over, when the new document names an instance of the same component with
/// the same id.
pub struct Instance<'v> {
    view: &'v View,
    index: usize,
}

impl Instance<'_> {
    /// The root control of the instance's component body.
    pub fn root(&self) -> &Handle {
        self.view.id(self.index)
    }

    /// A typed handle to the instance's state `name`, if `T` matches its type.
    pub fn state<T: StateValue>(&self, name: &str) -> Option<State<T>> {
        let program = &self.view.program.0.checked;
        let states = &program.templates[template_of(program, Some(self.index))].states;
        let index = states.iter().position(|(n, _, _)| n == name)?;
        T::accepts(&states[index].1).then(|| self.state_at(index))
    }

    /// A typed handle to the instance's state number `index`; `T` must match
    /// its type.
    pub fn state_at<T: StateValue>(&self, index: usize) -> State<T> {
        let program = &self.view.program.0.checked;
        let template = template_of(program, Some(self.index));
        debug_assert!(T::accepts(&program.templates[template].states[index].1));
        State {
            slot: self.view.slot(Some(self.index), index),
            marker: PhantomData,
        }
    }
}

/// A typed handle to a state of the entry document or of a named component
/// instance ([`Instance`]), as `ui!` views expose them.
///
/// Handles of one view share the state through every [`View::reload`] that
/// keeps a state of this name and type, in an instance with the same id and
/// component for an instance's state; after a reload that drops it, a handle
/// keeps a detached value that nothing displays.
pub struct State<T> {
    slot: Rc<Slot>,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            slot: self.slot.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: StateValue> State<T> {
    /// The current value.
    pub fn get(&self) -> T {
        T::from_data(self.slot.cell.borrow().get(None))
    }

    /// Assigns the state and updates every binding that reads it, then runs
    /// its change handlers. A value that does not fit the declared type, such
    /// as a record list of the wrong shape, is an error and changes nothing.
    pub fn set(&self, value: T) -> Result {
        let data = value.into_data()?;
        let program = self.slot.program.borrow().clone();
        let template = template_of(&program.0.checked, self.slot.owner.get());
        let ty = &program.0.checked.templates[template].states[self.slot.index.get()].1;
        if !conforms(&data, ty, &program.0.checked) {
            let message = format!("state requires {ty:?}");
            return Err(RuntimeError::new(Span::default(), message).into());
        }
        let cell = self.slot.cell.borrow().clone();
        cell.set(data)
    }

    /// Runs `handler` with the new value whenever the state changes, from
    /// markup event blocks, bindings' sources or Rust alike, after the
    /// bindings that read it have updated. It runs outside every UI borrow,
    /// like an event handler, and is not re-entered by changes it makes to
    /// this state itself; an error it returns stops that update and reaches
    /// whoever changed the state. The handler lives with the interface and
    /// follows the state through reloads; [`Watch::remove`] stops it. A
    /// handler that captures a handle of its own state keeps the state alive
    /// until it is removed.
    pub fn on_change<R: HandlerResult>(&self, mut handler: impl FnMut(T) -> R + 'static) -> Watch {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let run = move |data: &Data| handler(T::from_data(data.clone())).into_result();
        self.slot
            .cell
            .borrow()
            .watch((id, Rc::new(RefCell::new(run))));
        Watch {
            slot: Rc::downgrade(&self.slot),
            id,
        }
    }
}

impl<T: StateValue + Clone + PartialEq + 'static> State<T> {
    /// Links this state to a Rust [`Signal`] in both directions: the state
    /// takes the signal's value now, then each follows the other's changes,
    /// whether they come from markup event blocks, bindings' sources or Rust.
    /// A value that is already equal on the other side stops the echo.
    ///
    /// Assigning the signal's value now can fail like [`Self::set`]; later,
    /// an error from assigning a signal change to the state (a float that is
    /// not finite, or a failing markup reaction or handler) waits for the host
    /// like other signal handler errors (see [`aegle_ui::take_reaction_error`];
    /// the native `App` reports it to its error handler). The link follows the
    /// state through reloads that keep it. It keeps both the signal and the
    /// state alive until [`Binding::remove`], which ends both directions.
    ///
    /// ```
    /// # fn link(view: &aegle_loader::View) -> aegle_ui::Result {
    /// use aegle_ui::Signal;
    ///
    /// let count = Signal::new(3_i64);
    /// let state = view.state::<i64>("count").expect("an int state `count`");
    /// state.bind_signal(&count)?;
    /// count.set(4); // Markup bindings reading `count` update.
    /// # Ok(())
    /// # }
    /// ```
    pub fn bind_signal(&self, signal: &Signal<T>) -> Result<Binding> {
        self.set(signal.get())?;
        let model = signal.clone();
        let watch = self.on_change(move |value: T| model.set(value));
        let state = self.clone();
        let binding = signal.on_change(move |value: T| state.set(value));
        Ok(binding.on_remove(move || watch.remove()))
    }

    /// Makes this state follow a Rust [`Derived`] value one way: the state
    /// takes the value now and after each of its changes. A markup event
    /// block may still assign the state; its value stands until `source`
    /// next changes.
    ///
    /// Assigning the value now can fail like [`Self::set`]; a later failing
    /// assignment waits for the host like [`Self::bind_signal`]'s. The
    /// interface keeps `source` alive, so a temporary `Derived` may be passed;
    /// the link follows the state through reloads that keep it and ends with
    /// the interface or [`Binding::remove`], which also releases `source`.
    ///
    /// ```
    /// # fn link(view: &aegle_loader::View) -> aegle_ui::Result {
    /// use aegle_ui::Signal;
    ///
    /// let first = Signal::new(String::from("Ada"));
    /// let title = view.state::<String>("title").expect("a string state `title`");
    /// title.follow(&first.map(|name| format!("Hello, {name}")))?;
    /// first.set("Grace".into()); // `title` becomes "Hello, Grace".
    /// # Ok(())
    /// # }
    /// ```
    pub fn follow(&self, source: &Derived<T>) -> Result<Binding> {
        self.set(source.get())?;
        let held: Rc<Followed<T>> = Rc::new(RefCell::new(Some((source.clone(), self.clone()))));
        // The source owns its handlers, so this one holds the link weakly.
        let link = Rc::downgrade(&held);
        let binding = source.on_change(move |value: T| {
            let state = link
                .upgrade()
                .and_then(|held| Some(held.borrow().as_ref()?.1.clone()));
            state.map_or(Ok(()), |state| state.set(value))
        });
        let mut followed = self.slot.followed.borrow_mut();
        followed.retain(|held| held.strong_count() > 0);
        followed.push(Rc::downgrade(&held) as Weak<dyn Any>);
        let link = Rc::downgrade(&held);
        self.slot.root.borrow().keep_alive(held);
        Ok(binding.on_remove(move || {
            // Dropped outside the borrow: the source's drop may reach the state.
            let taken = link.upgrade().and_then(|held| held.borrow_mut().take());
            drop(taken);
        }))
    }
}

/// A registered state change handler; see [`State::on_change`]. Dropping it
/// keeps the handler.
#[derive(Clone, Debug)]
pub struct Watch {
    slot: Weak<Slot>,
    id: u64,
}

impl Watch {
    /// Unregisters the handler: it never runs again. Removing it twice, or
    /// after its state is gone, does nothing.
    pub fn remove(&self) {
        if let Some(slot) = self.slot.upgrade() {
            slot.cell.borrow().unwatch(self.id);
        }
    }
}

/// Rust types of markup states: `bool`, `i64`, `f32`, `String`, `Vec<i64>` and
/// `Vec<String>`, or [`Data`] for any type, including records and record lists.
pub trait StateValue: Sized {
    /// Whether a state of markup type `ty` can be held as `Self`.
    fn accepts(ty: &Type) -> bool;
    /// Converts a value of an accepted type.
    fn from_data(data: Data) -> Self;
    /// Converts for assignment; floats must be finite.
    fn into_data(self) -> Result<Data>;
}

macro_rules! state_value {
    ($ty:ty, $markup:expr, $data:pat => $from:expr, $value:ident => $into:expr) => {
        impl StateValue for $ty {
            fn accepts(ty: &Type) -> bool {
                *ty == $markup
            }
            fn from_data(data: Data) -> Self {
                match data {
                    $data => $from,
                    _ => unreachable!("state types are checked when the handle is created"),
                }
            }
            fn into_data(self) -> Result<Data> {
                let $value = self;
                Ok($into)
            }
        }
    };
}

impl StateValue for Data {
    fn accepts(_: &Type) -> bool {
        true
    }
    fn from_data(data: Data) -> Self {
        data
    }
    fn into_data(self) -> Result<Data> {
        Ok(self)
    }
}

state_value!(bool, Type::Bool, Data::Bool(v) => v, v => Data::Bool(v));
state_value!(i64, Type::Int, Data::Int(v) => v, v => Data::Int(v));
state_value!(String, Type::String, Data::String(v) => v.to_string(), v => Data::String(v.into()));
state_value!(f32, Type::Float, Data::Float(v) => v, v => {
    if !v.is_finite() {
        return Err(RuntimeError::new(Span::default(), "float states must be finite").into());
    }
    Data::Float(v)
});
state_value!(Vec<i64>, Type::List(Box::new(Type::Int)), Data::List(items) => {
    items.iter().map(|item| match item { Data::Int(v) => *v, _ => unreachable!() }).collect()
}, v => Data::List(v.into_iter().map(Data::Int).collect()));
state_value!(Vec<String>, Type::List(Box::new(Type::String)), Data::List(items) => {
    items.iter().map(|item| match item { Data::String(v) => v.to_string(), _ => unreachable!() }).collect()
}, v => Data::List(v.into_iter().map(|s| Data::String(s.into())).collect()));
