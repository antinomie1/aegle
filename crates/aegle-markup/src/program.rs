//! Checked dynamic programs: states, bindings, events, blocks and components.

mod instance;
mod steps;

use std::{collections::HashMap, rc::Rc};

use crate::checked::{Bound, Element, ElementKind, EventKind, HostCall, Program, Template};

use crate::schema::{allowed, kind as builtin, literal, property_name, valid_id, validate};
use crate::{
    Document, Error, Expr, ExprKind, Item, Kind, Node, Param, PropertyName, Record, Span, Type,
    Value,
};

/// Names visible to expressions while checking one template.
pub(crate) struct Scope {
    pub template: usize,
    pub params: Vec<(String, Type)>,
    pub states: Vec<(String, Type)>,
    pub items: Vec<(String, Type)>,
    /// `let` names and an event's carried value, innermost last.
    pub locals: Vec<(String, Type)>,
    /// The event source kind while checking a handler.
    pub source: Option<Kind>,
}

pub(crate) struct Checker {
    names: HashMap<String, usize>,
    signatures: Vec<Vec<(String, Type, bool)>>,
    ids: Vec<(String, Kind)>,
    uses: Vec<Vec<(usize, Span)>>,
    /// Inside an `if` or `for` block, where controls come and go.
    in_block: bool,
    pub(crate) records: Vec<Record>,
    pub(crate) record_index: HashMap<String, usize>,
    /// Declared events and whether a `slot` exists, per template.
    event_sigs: Vec<Vec<(String, Option<Type>)>>,
    slots: Vec<bool>,
    host_calls: Vec<HostCall>,
    /// File of the template being checked.
    file: usize,
    slot_used: bool,
}

/// Checks a program. `files[0]` is the entry document and must have a root;
/// other files only declare components, visible by name to every file.
/// Errors carry the index of the file they belong to.
pub fn check_program(files: Vec<Document>) -> Result<Program, (usize, Error)> {
    let none = Span { start: 0, end: 0 };
    let mut names = HashMap::new();
    let mut declared = Vec::new();
    let mut records: Vec<Record> = Vec::new();
    let mut record_index = HashMap::new();
    let mut entry = None;
    for (file, document) in files.into_iter().enumerate() {
        match (file, document.root) {
            (0, Some(root)) => entry = Some(root),
            (0, None) => return Err((0, Error::new(none, "the document has no root component"))),
            (_, Some(root)) => {
                return Err((
                    file,
                    Error::new(root.span, "imported files may only declare components"),
                ));
            }
            (_, None) => {}
        }
        for record in document.records {
            let error = |message: &str| (file, Error::new(record.span, message));
            if builtin(&record.name).is_some() || names.contains_key(&record.name) {
                return Err(error("a record cannot reuse a built-in or component name"));
            }
            if record_index
                .insert(record.name.clone(), records.len())
                .is_some()
            {
                return Err(error("duplicate record name"));
            }
            let mut seen = Vec::new();
            for (field, ty) in &record.fields {
                if seen.contains(&field) || !valid_id(field) {
                    return Err(error("record fields must be unique Rust identifiers"));
                }
                if !matches!(ty, Type::Bool | Type::Int | Type::Float | Type::String) {
                    return Err(error("record fields are bool, int, float or string"));
                }
                seen.push(field);
            }
            records.push(record);
        }
        for component in document.components {
            let error = |message: &str| (file, Error::new(component.span, message));
            if builtin(&component.name).is_some() {
                return Err(error("a component cannot reuse a built-in name"));
            }
            if record_index.contains_key(&component.name) {
                return Err(error("a component cannot reuse a record name"));
            }
            if names
                .insert(component.name.clone(), declared.len() + 1)
                .is_some()
            {
                return Err(error("duplicate component name"));
            }
            declared.push((file, component));
        }
    }
    let mut signatures = vec![Vec::new()];
    let mut event_sigs = vec![Vec::new()];
    let mut slots = vec![false];
    for (file, component) in &declared {
        let mut sig: Vec<(String, Option<Type>)> = Vec::new();
        for event in &component.events {
            let error = |message: &str| (*file, Error::new(event.span, message));
            if sig.iter().any(|(name, _)| *name == event.name) || !valid_id(&event.name) {
                return Err(error("event names must be unique Rust identifiers"));
            }
            if let Some(ty) = &event.ty {
                resolve(ty, &record_index).map_err(|m| error(&m))?;
            }
            sig.push((event.name.clone(), event.ty.clone()));
        }
        event_sigs.push(sig);
        slots.push(has_slot(&component.root));
    }
    for (file, component) in &declared {
        for param in &component.params {
            resolve(&param.ty, &record_index).map_err(|m| (*file, Error::new(param.span, m)))?;
        }
    }
    for (_, component) in &declared {
        let params = component.params.iter();
        signatures.push(
            params
                .map(|p| (p.name.clone(), p.ty.clone(), p.default.is_some()))
                .collect(),
        );
    }
    let mut checker = Checker {
        names,
        signatures,
        ids: Vec::new(),
        uses: vec![Vec::new(); declared.len() + 1],
        in_block: false,
        records,
        record_index,
        event_sigs,
        slots,
        host_calls: Vec::new(),
        file: 0,
        slot_used: false,
    };
    let root = entry.unwrap();
    let name = root.name.clone();
    let mut templates = vec![
        checker
            .template(0, name, Vec::new(), Vec::new(), root)
            .map_err(|e| (0, e))?,
    ];
    let files: Vec<usize> = std::iter::once(0)
        .chain(declared.iter().map(|(f, _)| *f))
        .collect();
    for (index, (file, component)) in declared.into_iter().enumerate() {
        checker.file = file;
        let template = checker.template(
            index + 1,
            component.name,
            component.params,
            component.events,
            component.root,
        );
        templates.push(template.map_err(|e| (file, e))?);
    }
    // Reject recursive instantiation, which would never finish building.
    let mut state = vec![0u8; templates.len()];
    fn visit(t: usize, uses: &[Vec<(usize, Span)>], state: &mut [u8]) -> Result<(), (usize, Span)> {
        state[t] = 1;
        for &(next, span) in &uses[t] {
            match state[next] {
                1 => return Err((t, span)),
                0 => visit(next, uses, state)?,
                _ => {}
            }
        }
        state[t] = 2;
        Ok(())
    }
    for t in 0..templates.len() {
        if state[t] == 0 {
            visit(t, &checker.uses, &mut state).map_err(|(t, span)| {
                (
                    files[t],
                    Error::new(span, "components cannot instantiate themselves recursively"),
                )
            })?;
        }
    }
    Ok(Program {
        templates,
        ids: checker.ids,
        records: checker.records,
        host_calls: checker.host_calls,
        files: Vec::new(),
    })
}

/// Whether a component body places its instance's children.
fn has_slot(node: &Node) -> bool {
    fn any(list: &[Item]) -> bool {
        list.iter().any(|item| match item {
            Item::Slot => true,
            Item::Node(node) => has_slot(node),
            Item::If(_, then, otherwise) => any(then) || any(otherwise),
            Item::For(_, _, _, body) => any(body),
        })
    }
    any(&node.children)
}

/// Rejects a record type that is not declared.
fn resolve(ty: &Type, records: &HashMap<String, usize>) -> Result<(), String> {
    match ty {
        Type::Record(name) if !records.contains_key(name) => Err(format!("unknown type `{name}`")),
        Type::List(item) => resolve(item, records),
        _ => Ok(()),
    }
}

impl Checker {
    fn template(
        &mut self,
        index: usize,
        name: String,
        params: Vec<Param>,
        events: Vec<crate::EventDecl>,
        mut root: Node,
    ) -> Result<Template, Error> {
        self.slot_used = false;
        let mut scope = Scope {
            template: index,
            params: Vec::new(),
            states: Vec::new(),
            items: Vec::new(),
            locals: Vec::new(),
            source: None,
        };
        let mut checked_params = Vec::new();
        for mut param in params {
            if scope.params.iter().any(|(name, _)| *name == param.name) {
                return Err(Error::new(param.span, "duplicate parameter"));
            }
            if let Some(default) = &mut param.default {
                if !constant(default) {
                    return Err(Error::new(
                        default.span,
                        "parameter defaults must be literals",
                    ));
                }
                self.expr(default, &scope, Some(&param.ty))?;
            }
            scope.params.push((param.name.clone(), param.ty.clone()));
            checked_params.push((param.name, param.ty, param.default));
        }
        let mut states = Vec::new();
        for mut state in std::mem::take(&mut root.states) {
            let taken = scope
                .params
                .iter()
                .chain(&scope.states)
                .any(|(n, _)| *n == state.name);
            if taken || !valid_id(&state.name) {
                return Err(Error::new(
                    state.span,
                    "state names must be unique Rust identifiers",
                ));
            }
            resolve(&state.ty, &self.record_index)
                .map_err(|message| Error::new(state.span, message))?;
            self.expr(&mut state.value, &scope, Some(&state.ty))?;
            scope.states.push((state.name.clone(), state.ty.clone()));
            states.push((state.name, state.ty, state.value));
        }
        let root = self.element(root, &mut scope, true)?;
        Ok(Template {
            name,
            params: checked_params,
            states,
            events: events.into_iter().map(|e| (e.name, e.ty)).collect(),
            slot: self.slot_used,
            file: self.file,
            root,
        })
    }

    /// Checks a node; `top` marks the template root.
    fn element(&mut self, node: Node, scope: &mut Scope, top: bool) -> Result<Element, Error> {
        if let Some(state) = node.states.first() {
            return Err(Error::new(
                state.span,
                "states belong on the root of a document or component",
            ));
        }
        if let Some(&template) = self.names.get(&node.name) {
            return self.instance(node, template, scope);
        }
        let kind = builtin(&node.name)
            .ok_or_else(|| Error::new(node.span, format!("unknown component `{}`", node.name)))?;
        if kind == Kind::Window && !(top && scope.template == 0) {
            return Err(Error::new(
                node.span,
                "Window is only allowed at the document root",
            ));
        }
        if !kind.is_container() && !node.children.is_empty() {
            return Err(Error::new(
                node.span,
                format!("{} does not accept children", node.name),
            ));
        }
        let mut element = Element {
            kind: ElementKind::Builtin(kind),
            id: None,
            properties: Vec::new(),
            arguments: Vec::new(),
            events: Vec::new(),
            handlers: Vec::new(),
            children: Vec::new(),
            slot: None,
            span: node.span,
        };
        for property in node.properties {
            let error = |message: String| Error::new(property.value_span, message);
            if property.name == "id" {
                let Value::Identifier(id) = property.value else {
                    return Err(error("id requires a Rust identifier".into()));
                };
                if scope.template != 0 || self.in_block {
                    return Err(error(
                        "ids are only available outside blocks and components".into(),
                    ));
                }
                if element.id.is_some() || !valid_id(&id) || self.ids.iter().any(|(n, _)| *n == id)
                {
                    return Err(error(format!("invalid, duplicate or reserved id `{id}`")));
                }
                element.id = Some(self.ids.len());
                self.ids.push((id, kind));
                continue;
            }
            let name = property_name(&property.name)
                .ok_or_else(|| error(format!("unknown property `{}`", property.name)))?;
            if element.properties.iter().any(|(n, _)| *n == name) {
                return Err(error(format!("duplicate property `{}`", property.name)));
            }
            let value = match property.value {
                Value::Identifier(id) if !identifier_literal(name) => {
                    let span = property.value_span;
                    Err(Expr {
                        kind: ExprKind::Name(id),
                        span,
                    })
                }
                Value::Expr(expr) => Err(*expr),
                value => Ok(literal(value)),
            };
            let value = match value {
                Ok(value) => {
                    validate(kind, name, &value).map_err(error)?;
                    Bound::Literal(value)
                }
                Err(mut expr) => {
                    let ty = bindable(name)
                        .filter(|_| allowed(kind, name))
                        .ok_or_else(|| {
                            error(format!("{name:?} on {kind:?} accepts only literal values"))
                        })?;
                    self.expr(&mut expr, scope, Some(&ty))?;
                    Bound::Expr(Rc::new(expr))
                }
            };
            element.properties.push((name, value));
        }
        let literals = element
            .properties
            .iter()
            .filter_map(|(name, value)| match value {
                Bound::Literal(value) => Some((*name, value)),
                Bound::Expr(_) => None,
            });
        crate::schema::validate_range(kind, literals, node.span)?;
        let has = |name| element.properties.iter().any(|(n, _)| *n == name);
        if has(PropertyName::Easing) && !has(PropertyName::Transition) {
            return Err(Error::new(
                node.span,
                "easing requires a transition duration on the same component",
            ));
        }
        for event in node.events {
            let event_kind = match (kind, event.name.as_str()) {
                (Kind::Button, "clicked") => EventKind::Clicked,
                (Kind::CheckBox | Kind::Switch | Kind::RadioButton | Kind::Slider, "changed") => {
                    EventKind::Changed
                }
                (Kind::TextField, "submitted") => EventKind::Submitted,
                _ => {
                    return Err(Error::new(
                        event.span,
                        format!("{kind:?} has no event `{}`", event.name),
                    ));
                }
            };
            if element.events.iter().any(|(k, _)| *k == event_kind) {
                return Err(Error::new(event.span, "duplicate event handler"));
            }
            if event.param.is_some() {
                return Err(Error::new(event.span, "built-in events carry no value"));
            }
            scope.source = Some(kind);
            let steps = self.steps(event.body, scope);
            scope.source = None;
            element.events.push((event_kind, steps?.into()));
        }
        for item in node.children {
            element.children.push(self.child(item, scope)?);
        }
        Ok(element)
    }
}

/// Properties whose values may be expressions, with their types.
fn bindable(name: PropertyName) -> Option<Type> {
    use PropertyName::*;
    match name {
        Text | Label => Some(Type::String),
        Visible | Enabled | Checked | Mixed | ReadOnly => Some(Type::Bool),
        Value => Some(Type::Float),
        _ => None,
    }
}

/// Properties that take bare identifiers as enum values.
fn identifier_literal(name: PropertyName) -> bool {
    use PropertyName::*;
    matches!(name, Width | Height | Theme | Easing)
}

/// Literals and lists of literals.
fn constant(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(_) => true,
        ExprKind::List(items) => items.iter().all(constant),
        _ => false,
    }
}
