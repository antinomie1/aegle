//! Built interfaces: root, named controls, entry states and reloading.

use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::{Rc, Weak},
    sync::atomic::{AtomicU64, Ordering},
};

use aegle_markup::{ElementKind, Span, Type};
use aegle_ui::{Binding, Container, HandlerResult, Result, Signal};

use crate::{
    Data, Program, RuntimeError,
    build::{self, Block},
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
    ids: Vec<Option<Handle>>,
    /// Effects of the entry document, also kept alive by the root control.
    effects: Rc<RefCell<Block>>,
    /// Top-level controls of a window's content, replaced by a reload.
    content: Vec<Handle>,
    /// Container of a fragment root.
    parent: Option<Container>,
    /// Entry states that typed handles share, repointed by a reload.
    slots: RefCell<Vec<Option<Rc<Slot>>>>,
    /// A Window document's role and output, which a reload keeps.
    #[cfg(feature = "window")]
    surface: Option<crate::window::Surface>,
}

type Carried<'a> = &'a dyn Fn(&str, &Type) -> Option<Data>;

pub(crate) fn fragment(program: &Program, parent: &Container, carried: Carried) -> Result<View> {
    let root = &program.0.checked.templates[0].root;
    if root.kind == ElementKind::Window {
        return Err("a Window document is opened with Program::open".into());
    }
    crate::actions::validate(&program.0.checked)?;
    let env = Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), carried)?;
    let mut ids = vec![None; program.0.checked.ids.len()];
    let (mut block, mut created) = (Block::new(), Vec::new());
    let result = build::root(root, parent, &env, &mut block, &mut created, &mut ids);
    if let Err(error) = result {
        if let Some(root) = created.first() {
            root.node().remove();
        }
        return Err(error);
    }
    let root = created.swap_remove(0);
    let _ = env.scope.named.set(ids.as_slice().into());
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
        ids: Vec::new(),
        effects: effects.clone(),
        content: Vec::new(),
        parent: None,
        slots: RefCell::new(Vec::new()),
        surface: Some(surface),
    };
    if let Err(error) = view.fill(program, view.env.clone()) {
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
        self.ids[index].as_ref()
    }

    /// The control with entry ID number `index`, in declaration order.
    pub fn id(&self, index: usize) -> &Handle {
        self.ids[index]
            .as_ref()
            .expect("entry IDs are built with the view")
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
        let mut slots = self.slots.borrow_mut();
        if slots.len() <= index {
            slots.resize(index + 1, None);
        }
        let slot = slots[index].get_or_insert_with(|| {
            Rc::new(Slot {
                cell: RefCell::new(self.env.scope.states[index].clone()),
                program: RefCell::new(self.program.clone()),
                index: std::cell::Cell::new(index),
            })
        });
        State {
            slot: slot.clone(),
            marker: PhantomData,
        }
    }

    /// Hands every typed state handle to the state of the same name and type
    /// in `next`, with its change handlers; others keep their detached state.
    fn hand_over(&self, next: &Program, env: &Env) -> Vec<Option<Rc<Slot>>> {
        let states = &next.0.checked.templates[0].states;
        let mut slots = vec![None; states.len()];
        let old = &self.program.0.checked.templates[0].states;
        for slot in self.slots.borrow().iter().flatten() {
            let (name, ty, _) = &old[slot.index.get()];
            let Some(index) = states.iter().position(|(n, t, _)| n == name && t == ty) else {
                continue;
            };
            let cell = env.scope.states[index].clone();
            cell.adopt(&slot.cell.borrow());
            *slot.cell.borrow_mut() = cell;
            *slot.program.borrow_mut() = next.clone();
            slot.index.set(index);
            slots[index] = Some(slot.clone());
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
        let states = &self.program.0.checked.templates[0].states;
        let carried = |name: &str, ty: &Type| {
            let index = states.iter().position(|(n, t, _)| n == name && t == ty)?;
            Some(self.env.scope.states[index].get(None))
        };
        let window = self.program.0.checked.templates[0].root.kind == ElementKind::Window;
        let fresh = program.0.checked.templates[0].root.kind == ElementKind::Window;
        if window != fresh {
            return Err("a reload cannot change whether the root is a Window".into());
        }
        if let Some(parent) = &self.parent {
            let view = fragment(program, parent, &carried)?;
            *view.slots.borrow_mut() = self.hand_over(program, &view.env);
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
        let env = Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), &carried)?;
        let old = std::mem::take(&mut self.content);
        let (old_ids, old_block) = (std::mem::take(&mut self.ids), self.effects.take());
        let previous = self.program.clone();
        if let Err(error) = self.fill(program, env) {
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
        let slots = self.hand_over(&current, &self.env);
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
    fn fill(&mut self, program: &Program, env: Env) -> Result {
        let root = &program.0.checked.templates[0].root;
        let mut ids = vec![None; program.0.checked.ids.len()];
        if let Some(index) = root.id {
            ids[index] = Some(self.root.clone());
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
        )?;
        *self.effects.borrow_mut() = block;
        let _ = env.scope.named.set(ids.as_slice().into());
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

/// One entry state as its typed handles see it. A reload repoints it to
/// the new interface's state of the same name and type.
pub(crate) struct Slot {
    cell: RefCell<Rc<Cell>>,
    /// The program whose records a value is checked against.
    program: RefCell<Program>,
    /// The state's index in that program's entry document.
    index: std::cell::Cell<usize>,
}

/// A typed handle to an entry-document state, as `ui!` views expose them.
///
/// Handles of one view share the state through every [`View::reload`] that
/// keeps a state of this name and type; after a reload that drops it, a
/// handle keeps a detached value that nothing displays.
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
        let ty = &program.0.checked.templates[0].states[self.slot.index.get()].1;
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
