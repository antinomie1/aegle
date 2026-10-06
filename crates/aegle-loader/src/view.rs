//! Built interfaces: root, named controls, entry states and reloading.

use std::{cell::RefCell, marker::PhantomData, rc::Rc};

use aegle_markup::{ElementKind, Kind, Span, Type};
use aegle_ui::{Container, Result};

use crate::{
    Data, Program, RuntimeError,
    build::{self, Block, Parent},
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
}

type Carried<'a> = &'a dyn Fn(&str, &Type) -> Option<Data>;

pub(crate) fn fragment(program: &Program, parent: &Container, carried: Carried) -> Result<View> {
    let root = &program.0.checked.templates[0].root;
    if root.kind == ElementKind::Builtin(Kind::Window) {
        return Err("a Window document is opened with Program::open".into());
    }
    program.0.actions.validate(&program.0.checked)?;
    let env = Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), carried)?;
    let mut ids = vec![None; program.0.checked.ids.len()];
    let (mut block, mut created) = (Block::new(), Vec::new());
    let host = Parent { container: parent };
    let result = build::element(root, &host, &env, &mut block, &mut created, Some(&mut ids));
    if let Err(error) = result {
        if let Some(root) = created.first() {
            root.node().remove()?;
        }
        return Err(error);
    }
    let root = created.swap_remove(0);
    let effects = Rc::new(RefCell::new(block));
    root.node().keep_alive(effects.clone())?;
    Ok(View {
        program: program.clone(),
        root,
        env,
        ids,
        effects,
        content: Vec::new(),
        parent: Some(parent.clone()),
    })
}

#[cfg(any(
    all(feature = "wayland", target_os = "linux"),
    all(feature = "windows", target_os = "windows")
))]
pub(crate) fn window(program: &Program, app: &aegle_app::App) -> Result<View> {
    use aegle_markup::{Bound, PropertyName, Value};
    let root = &program.0.checked.templates[0].root;
    if root.kind != ElementKind::Builtin(Kind::Window) {
        return Err("Program::open requires a Window document root".into());
    }
    program.0.actions.validate(&program.0.checked)?;
    let literal = |name| {
        root.properties.iter().find_map(|(n, bound)| match bound {
            Bound::Literal(value) if *n == name => Some(value),
            _ => None,
        })
    };
    let title = match literal(PropertyName::Title) {
        Some(Value::String(title)) => title.as_str(),
        _ => "Aegle",
    };
    let mut options = aegle_app::WindowOptions::default();
    if let Some(Value::Length(width)) = literal(PropertyName::Width) {
        options.width = *width as u32;
    }
    if let Some(Value::Length(height)) = literal(PropertyName::Height) {
        options.height = *height as u32;
    }
    let window = app.window_with_options(title, options)?;
    let handle = Handle::Window(window.clone());
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
    };
    if let Err(error) = view.fill(program, view.env.clone()) {
        window.close()?;
        return Err(error);
    }
    handle.node().keep_alive(effects)?;
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
        let ty = self.program.0.checked.templates[0].states[index].1.clone();
        debug_assert!(T::accepts(&ty));
        State {
            cell: self.env.scope.states[index].clone(),
            ty,
            program: self.program.clone(),
            marker: PhantomData,
        }
    }

    /// Replaces this interface with one built from `program`, atomically: the
    /// new interface is fully built before the old one is removed, and a
    /// failure keeps the old one. Entry states with the same name and type
    /// keep their values; other control-local state is reset. A Window
    /// document keeps its native window, title and size, and rebuilds its
    /// content; window properties the new version omits keep their values.
    pub fn reload(&mut self, program: &Program) -> Result {
        program.0.actions.inherit(&self.program.0.actions);
        let states = &self.program.0.checked.templates[0].states;
        let carried = |name: &str, ty: &Type| {
            let index = states.iter().position(|(n, t, _)| n == name && t == ty)?;
            Some(self.env.scope.states[index].get(None))
        };
        let window =
            self.program.0.checked.templates[0].root.kind == ElementKind::Builtin(Kind::Window);
        let fresh = program.0.checked.templates[0].root.kind == ElementKind::Builtin(Kind::Window);
        if window != fresh {
            return Err("a reload cannot change whether the root is a Window".into());
        }
        if let Some(parent) = &self.parent {
            let view = fragment(program, parent, &carried)?;
            self.root.node().remove()?;
            *self = view;
            return Ok(());
        }
        let env = Env::instantiate(program.0.clone(), 0, Vec::new(), Vec::new(), &carried)?;
        let old = std::mem::take(&mut self.content);
        let (old_ids, old_block) = (std::mem::take(&mut self.ids), self.effects.take());
        if let Err(error) = self.fill(program, env) {
            for handle in std::mem::replace(&mut self.content, old) {
                handle.node().remove()?;
            }
            self.ids = old_ids;
            *self.effects.borrow_mut() = old_block;
            return Err(error);
        }
        for handle in old {
            handle.node().remove()?;
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
        let mut block = Block::new();
        let parent = Parent::of(&self.root);
        let result = build::children(
            &root.children,
            &parent,
            &env,
            &mut block,
            &mut self.content,
            Some(&mut ids),
        )
        .and_then(|()| build::decorate(root, &self.root, &env, &mut block));
        result?;
        *self.effects.borrow_mut() = block;
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

/// A typed handle to an entry-document state, as `ui!` views expose them.
pub struct State<T> {
    cell: Rc<Cell>,
    ty: Type,
    program: Program,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self {
            cell: self.cell.clone(),
            ty: self.ty.clone(),
            program: self.program.clone(),
            marker: PhantomData,
        }
    }
}

impl<T: StateValue> State<T> {
    /// The current value.
    pub fn get(&self) -> T {
        T::from_data(self.cell.get(None))
    }

    /// Assigns the state and updates every binding that reads it. A value that
    /// does not fit the declared type, such as a record list of the wrong shape,
    /// is an error and changes nothing.
    pub fn set(&self, value: T) -> Result {
        let data = value.into_data()?;
        if !conforms(&data, &self.ty, &self.program.0.checked) {
            let message = format!("state requires {:?}", self.ty);
            return Err(RuntimeError::new(Span::default(), message).into());
        }
        self.cell.set(data)
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
