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

/// Where children are appended.
pub(crate) struct Parent<'a> {
    pub container: &'a Container,
}

impl<'a> Parent<'a> {
    pub fn of(handle: &'a Handle) -> Self {
        Self {
            container: handle.container(),
        }
    }
}

/// Builds `children` into `parent`, recording the top-level handles created.
/// `ids` collects entry-level named controls outside blocks and components.
pub(crate) fn children(
    children: &[Child],
    parent: &Parent,
    env: &Env,
    block: &mut Block,
    created: &mut Vec<Handle>,
    mut ids: Option<&mut [Option<Handle>]>,
) -> Result {
    for child in children {
        match child {
            Child::Element(child) => {
                element(child, parent, env, block, created, ids.as_deref_mut())?
            }
            Child::If(condition, then, otherwise) => {
                let wrapper = wrapper(parent)?;
                created.push(Handle::Container(wrapper.clone()));
                conditional(
                    condition.clone(),
                    then.clone(),
                    otherwise.clone(),
                    wrapper,
                    env,
                    block,
                )?;
            }
            Child::For(list, key, body) => {
                let wrapper = wrapper(parent)?;
                created.push(Handle::Container(wrapper.clone()));
                repeat(
                    (list.clone(), key.clone()),
                    body.clone(),
                    wrapper,
                    env,
                    block,
                )?;
            }
            Child::Slot => {
                if let Some(slot) = &env.slot {
                    self::children(&slot.children, parent, &slot.env, block, created, None)?;
                }
            }
        }
    }
    Ok(())
}

/// Builds one element or component instance. Its control is pushed to
/// `created` as soon as it exists, so a failed build can still be removed.
pub(crate) fn element(
    element: &Element,
    parent: &Parent,
    env: &Env,
    block: &mut Block,
    created: &mut Vec<Handle>,
    mut ids: Option<&mut [Option<Handle>]>,
) -> Result {
    let kind = match element.kind {
        ElementKind::Builtin(kind) => kind,
        ElementKind::Component(template) => {
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
            let mut inner =
                Env::instantiate(env.shared.clone(), template, params, emits, &|_, _| None)?;
            inner.slot = element.slot.as_ref().map(|children| {
                Rc::new(Slot {
                    children: children.clone(),
                    env: env.clone(),
                })
            });
            let root = &program.templates[template].root;
            return self::element(root, parent, &inner, block, created, None);
        }
    };
    let handle = create(kind, element, parent.container)?;
    created.push(handle.clone());
    if let (Some(index), Some(ids)) = (element.id, ids.as_deref_mut()) {
        ids[index] = Some(handle.clone());
    }
    populate(element, &handle, env, block, ids)
}

/// Builds children, then applies properties, bindings, transitions and events.
pub(crate) fn populate(
    element: &Element,
    handle: &Handle,
    env: &Env,
    block: &mut Block,
    mut ids: Option<&mut [Option<Handle>]>,
) -> Result {
    if let Handle::Splitter(splitter) = handle {
        // Its two checked children fill the two panes.
        for (child, pane) in element
            .children
            .iter()
            .zip([splitter.first(), splitter.second()])
        {
            let parent = Parent { container: pane };
            let child = std::slice::from_ref(child);
            children(
                child,
                &parent,
                env,
                block,
                &mut Vec::new(),
                ids.as_deref_mut(),
            )?;
        }
    } else if !element.children.is_empty() {
        let parent = Parent::of(handle);
        children(&element.children, &parent, env, block, &mut Vec::new(), ids)?;
    }
    decorate(element, handle, env, block)
}

/// Applies an element's own properties, bindings, transition and events.
pub(crate) fn decorate(element: &Element, handle: &Handle, env: &Env, block: &mut Block) -> Result {
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
    crate::motion::transitions(element, handle)?;
    for (event, steps) in &element.events {
        listen(handle, *event, steps.clone(), env.clone())?;
    }
    Ok(())
}

fn binding(handle: Handle, name: PropertyName, expr: Rc<Expr>, env: Env) -> Result<Rc<Effect>> {
    let mut last = None;
    Effect::new(move |effect| {
        // A control removed directly by the application no longer updates.
        if !handle.node().is_alive() {
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

/// A transparent group holding a block's children in the parent's own layout.
fn wrapper(parent: &Parent) -> Result<Container> {
    parent.container.contents()
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
        if !wrapper.is_alive() {
            return Ok(());
        }
        let value = truth(eval(&condition, &env, Some(effect), None).map_err(|e| env.locate(e))?);
        if shown == Some(value) {
            return Ok(());
        }
        shown = None;
        clear(&mut handles, &mut owned)?;
        let parent = Parent {
            container: &wrapper,
        };
        let items = if value { &then } else { &otherwise };
        children(items, &parent, &env, &mut owned, &mut handles, None)?;
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
        if !wrapper.is_alive() {
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
        let parent = Parent {
            container: &wrapper,
        };
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
            let result = children(&body, &parent, &env, &mut row.block, &mut row.handles, None);
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
