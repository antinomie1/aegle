//! Building elements, component instances, bindings and structural blocks.

use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use aegle_markup::{Bound, Child, Element, ElementKind, Expr, PropertyName, Value};
use aegle_ui::{Container, Result};

use crate::{
    Data, RuntimeError,
    eval::{Emit, Env, Param, Slot, eval, truth},
    handle::{Handle, apply, consumed, create, listen},
    reactive::Effect,
};

/// Effects owned by one built region; dropping it stops their updates.
pub(crate) type Block = Vec<Rc<Effect>>;

/// The passes over one built region, in document order: every control is
/// created, then properties, bindings and nested blocks apply, then
/// transitions and events are installed. `ui!` emits the same order for static
/// documents, so containers see their children when their properties apply and
/// no initial value, including one inherited from an ancestor, animates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Create,
    Apply,
    Finish,
}

/// What the creation pass made, in walk order, revisited by the later passes.
#[derive(Clone)]
enum Made {
    Control(Handle),
    Block(Container),
    Instance(Env),
}

/// One region: a view, an `if` branch or a `for` row.
struct Region<'r> {
    pass: Pass,
    made: Vec<Made>,
    next: usize,
    block: &'r mut Block,
    /// Top-level controls, recorded as soon as they exist so a failed build
    /// can still remove them.
    created: &'r mut Vec<Handle>,
    /// Entry-level named controls, outside blocks and components.
    ids: Option<&'r mut [Option<Handle>]>,
}

/// Builds `children` into `parent` as one region.
pub(crate) fn children(
    children: &[Child],
    parent: &Container,
    env: &Env,
    block: &mut Block,
    created: &mut Vec<Handle>,
) -> Result {
    Region::run(block, created, None, |region| {
        region.children(children, parent, env, true)
    })
}

/// Builds the entry root `element` into `parent` as one region.
pub(crate) fn root(
    element: &Element,
    parent: &Container,
    env: &Env,
    block: &mut Block,
    created: &mut Vec<Handle>,
    ids: &mut [Option<Handle>],
) -> Result {
    Region::run(block, created, Some(ids), |region| {
        region.element(element, parent, env, true)
    })
}

/// Builds the entry root `element` of an open `window` as one region: the
/// window's properties, then its content, recorded in `content`.
pub(crate) fn window(
    element: &Element,
    window: &Handle,
    env: &Env,
    block: &mut Block,
    content: &mut Vec<Handle>,
    ids: &mut [Option<Handle>],
) -> Result {
    Region::run(block, content, Some(ids), |region| {
        region.control(element, window, env, true)
    })
}

impl<'r> Region<'r> {
    fn run(
        block: &'r mut Block,
        created: &'r mut Vec<Handle>,
        ids: Option<&'r mut [Option<Handle>]>,
        walk: impl Fn(&mut Self) -> Result,
    ) -> Result {
        let mut region = Self {
            pass: Pass::Create,
            made: Vec::new(),
            next: 0,
            block,
            created,
            ids,
        };
        for pass in [Pass::Create, Pass::Apply, Pass::Finish] {
            (region.pass, region.next) = (pass, 0);
            walk(&mut region)?;
        }
        Ok(())
    }

    /// Records what the creation pass made, or replays it in a later pass.
    fn made(&mut self, make: impl FnOnce(&mut Self) -> Result<Made>) -> Result<Made> {
        if self.pass == Pass::Create {
            let made = make(self)?;
            self.made.push(made.clone());
            return Ok(made);
        }
        self.next += 1;
        Ok(self.made[self.next - 1].clone())
    }

    fn children(&mut self, children: &[Child], parent: &Container, env: &Env, top: bool) -> Result {
        for child in children {
            match child {
                Child::Element(element) => self.element(element, parent, env, top)?,
                Child::If(..) | Child::For(..) => {
                    let Made::Block(wrapper) = self.made(|region| {
                        // A transparent group keeps the block in the parent's layout.
                        let wrapper = parent.contents()?;
                        if top {
                            region.created.push(Handle::Container(wrapper.clone()));
                        }
                        Ok(Made::Block(wrapper))
                    })?
                    else {
                        unreachable!("replayed in creation order")
                    };
                    match (self.pass, child) {
                        (Pass::Apply, Child::If(condition, then, otherwise)) => conditional(
                            condition.clone(),
                            then.clone(),
                            otherwise.clone(),
                            wrapper,
                            env,
                            self.block,
                        )?,
                        (Pass::Apply, Child::For(list, key, body)) => repeat(
                            (list.clone(), key.clone()),
                            body.clone(),
                            wrapper,
                            env,
                            self.block,
                        )?,
                        _ => {}
                    }
                }
                Child::Slot => {
                    if let Some(slot) = &env.slot {
                        self.children(&slot.children, parent, &slot.env, top)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Visits one element or component instance.
    fn element(&mut self, element: &Element, parent: &Container, env: &Env, top: bool) -> Result {
        let kind = match element.kind {
            ElementKind::Builtin(kind) => kind,
            ElementKind::Component(template) => {
                let Made::Instance(inner) =
                    self.made(|_| instance(element, template, env).map(Made::Instance))?
                else {
                    unreachable!("replayed in creation order")
                };
                let root = &env.shared.checked.templates[template].root;
                return self.element(root, parent, &inner, top);
            }
        };
        let Made::Control(handle) = self.made(|region| {
            let handle = create(kind, element, parent)?;
            if top {
                region.created.push(handle.clone());
            }
            if let (Some(index), Some(ids)) = (element.id, region.ids.as_deref_mut()) {
                ids[index] = Some(handle.clone());
            }
            Ok(Made::Control(handle))
        })?
        else {
            unreachable!("replayed in creation order")
        };
        self.control(element, &handle, env, false)
    }

    /// Runs this pass for an existing control, then for its children; `top`
    /// marks the children as top-level controls of the region.
    fn control(&mut self, element: &Element, handle: &Handle, env: &Env, top: bool) -> Result {
        match self.pass {
            Pass::Create => {}
            Pass::Apply => properties(element, handle, env, self.block)?,
            Pass::Finish => {
                crate::motion::transitions(element, handle)?;
                for (event, steps) in &element.events {
                    listen(handle, *event, steps.clone(), env.clone())?;
                }
            }
        }
        if let Handle::Splitter(splitter) = handle {
            // Its two checked children fill the two panes.
            for (child, pane) in element
                .children
                .iter()
                .zip([splitter.first(), splitter.second()])
            {
                self.children(std::slice::from_ref(child), pane, env, top)?;
            }
        } else if !element.children.is_empty() {
            self.children(&element.children, handle.container(), env, top)?;
        }
        Ok(())
    }
}

/// Creates a component instance's environment: its arguments, event handlers,
/// slot children and states.
fn instance(element: &Element, template: usize, env: &Env) -> Result<Env> {
    let program = &env.shared.checked;
    let params = program.templates[template]
        .params
        .iter()
        .zip(&element.arguments);
    let params = params
        .map(|((_, _, default), argument)| match argument {
            Some(expr) => Ok(Param::Bound(expr.clone(), env.clone())),
            None => eval(default.as_ref().unwrap(), env, None, None)
                .map(Param::Value)
                .map_err(|error| env.locate(error)),
        })
        .collect::<Result<_>>()?;
    let emits = (0..program.templates[template].events.len())
        .map(|event| {
            element
                .handlers
                .iter()
                .find(|h| h.event == event)
                .map(|h| Emit {
                    steps: h.steps.clone(),
                    binds_value: h.binds_value,
                    env: env.clone(),
                })
        })
        .collect();
    let mut inner = Env::instantiate(env.shared.clone(), template, params, emits, &|_, _| None)?;
    inner.slot = element.slot.as_ref().map(|children| {
        Rc::new(Slot {
            children: children.clone(),
            env: env.clone(),
        })
    });
    Ok(inner)
}

/// Applies an element's own literal properties and bindings.
fn properties(element: &Element, handle: &Handle, env: &Env, block: &mut Block) -> Result {
    let ElementKind::Builtin(kind) = element.kind else {
        unreachable!("component instances are expanded")
    };
    for (name, bound) in &element.properties {
        match bound {
            Bound::Literal(value) if !consumed(kind, *name) => apply(handle, *name, value)?,
            Bound::Literal(_) => {}
            Bound::Expr(expr) => {
                block.push(binding(handle.clone(), *name, expr.clone(), env.clone())?)
            }
        }
    }
    Ok(())
}

fn binding(handle: Handle, name: PropertyName, expr: Rc<Expr>, env: Env) -> Result<Rc<Effect>> {
    let mut last = None;
    Effect::new(move |effect| {
        // A control removed directly by the application no longer updates.
        if !handle.node().is_alive()? {
            return Ok(());
        }
        let value = eval(&expr, &env, Some(effect), None).map_err(|e| env.locate(e))?;
        if last.as_ref() != Some(&value) {
            let literal = match &value {
                Data::Bool(value) => Value::Bool(*value),
                Data::Float(value) => Value::Number(*value),
                Data::String(text) => Value::String(text.to_string()),
                Data::Int(_) | Data::List(_) | Data::Record(_) => {
                    unreachable!("checked binding types")
                }
            };
            apply(&handle, name, &literal)?;
            last = Some(value);
        }
        Ok(())
    })
}

/// Removes built controls and drops the effects that updated them.
fn clear(handles: &mut Vec<Handle>, block: &mut Block) -> Result {
    for handle in handles.drain(..) {
        handle.node().remove()?;
    }
    block.clear();
    Ok(())
}

fn conditional(
    condition: Rc<Expr>,
    then: Rc<[Child]>,
    otherwise: Rc<[Child]>,
    wrapper: Container,
    env: &Env,
    block: &mut Block,
) -> Result {
    let env = env.clone();
    let (mut shown, mut handles, mut owned) = (None, Vec::new(), Block::new());
    block.push(Effect::new(move |effect| {
        if !wrapper.is_alive()? {
            return Ok(());
        }
        let value = truth(eval(&condition, &env, Some(effect), None).map_err(|e| env.locate(e))?);
        if shown == Some(value) {
            return Ok(());
        }
        shown = None;
        clear(&mut handles, &mut owned)?;
        let items = if value { &then } else { &otherwise };
        children(items, &wrapper, &env, &mut owned, &mut handles)?;
        wrapper.set_visible(!handles.is_empty())?;
        shown = Some(value);
        Ok(())
    })?);
    Ok(())
}

/// A `for` key: ints or strings.
#[derive(Clone, Hash, PartialEq, Eq)]
enum Key {
    Int(i64),
    String(Rc<str>),
}

impl From<&Data> for Key {
    fn from(data: &Data) -> Self {
        match data {
            Data::Int(value) => Self::Int(*value),
            Data::String(value) => Self::String(value.clone()),
            _ => unreachable!("checked key types"),
        }
    }
}

struct Row {
    key: Key,
    item: Data,
    handles: Vec<Handle>,
    block: Block,
}

fn repeat(
    (list, key): (Rc<Expr>, Option<Rc<Expr>>),
    body: Rc<[Child]>,
    wrapper: Container,
    env: &Env,
    block: &mut Block,
) -> Result {
    let env = env.clone();
    let mut rows: Vec<Row> = Vec::new();
    block.push(Effect::new(move |effect| {
        if !wrapper.is_alive()? {
            return Ok(());
        }
        let Data::List(items) = eval(&list, &env, Some(effect), None).map_err(|e| env.locate(e))?
        else {
            unreachable!("checked list type")
        };
        if items.len() > env.shared.limits.get().rows {
            let message = "list exceeds the row limit; the list is unchanged";
            return Err(env.locate(RuntimeError::new(list.span, message).into()));
        }
        let mut keyed = Vec::with_capacity(items.len());
        let mut keys = HashSet::with_capacity(items.len());
        for item in items.iter() {
            let item_key = match &key {
                Some(key) => {
                    let inner = env.with_item(item.clone());
                    eval(key, &inner, Some(effect), None).map_err(|e| env.locate(e))?
                }
                None => item.clone(),
            };
            let item_key = Key::from(&item_key);
            if !keys.insert(item_key.clone()) {
                let message = "duplicate for key; the list is unchanged";
                return Err(env.locate(RuntimeError::new(list.span, message).into()));
            }
            keyed.push((item_key, item));
        }
        // Retained rows keep their controls and local state; new rows are built
        // at the end, so any other order needs one reparenting pass. A row whose
        // item value changed under the same key is rebuilt.
        let mut old: HashMap<Key, (usize, Row)> = rows
            .drain(..)
            .enumerate()
            .map(|(index, row)| (row.key.clone(), (index, row)))
            .collect();
        let (mut previous, mut built, mut moved) = (None, false, false);
        for (item_key, item) in keyed {
            if let Some((index, mut retained)) = old.remove(&item_key) {
                if retained.item == *item {
                    moved |= built || previous.is_some_and(|previous| index < previous);
                    previous = Some(index);
                    rows.push(retained);
                    continue;
                }
                clear(&mut retained.handles, &mut retained.block)?;
            }
            built = true;
            let mut row = Row {
                key: item_key.clone(),
                item: item.clone(),
                handles: Vec::new(),
                block: Block::new(),
            };
            let env = env.with_item(item.clone());
            let result = children(&body, &wrapper, &env, &mut row.block, &mut row.handles);
            rows.push(row);
            if let Err(error) = result {
                // Rows not yet visited keep their controls and stay tracked.
                rows.extend(old.into_values().map(|(_, row)| row));
                return Err(error);
            }
        }
        for (_, (_, mut row)) in old {
            clear(&mut row.handles, &mut row.block)?;
        }
        if moved {
            for handle in rows.iter().flat_map(|row| &row.handles) {
                handle.node().reparent(&wrapper)?;
            }
        }
        wrapper.set_visible(!rows.is_empty())
    })?);
    Ok(())
}
