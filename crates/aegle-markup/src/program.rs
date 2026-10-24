//! Checked dynamic programs: states, bindings, events, blocks and components.

mod element;
mod instance;
mod steps;

use std::collections::HashMap;

use crate::checked::{HostCall, Program, Template};
use crate::schema::valid_id;
use crate::{Document, ElementSpec, Error, Expr, ExprKind, Item, Node, Param, Record, Span, Type};

/// Names visible to expressions while checking one template.
pub(crate) struct Scope {
    pub template: usize,
    pub params: Vec<(String, Type)>,
    pub states: Vec<(String, Type)>,
    pub items: Vec<(String, Type)>,
    /// `let` names and an event's carried value, innermost last.
    pub locals: Vec<(String, Type)>,
    /// The spec of the element raising the event while checking a handler.
    pub source: Option<usize>,
}

pub(crate) struct Checker<'s> {
    names: HashMap<String, usize>,
    signatures: Vec<Vec<(String, Type, bool)>>,
    ids: Vec<(String, crate::ElementKind)>,
    pub(crate) specs: &'s [ElementSpec<'s>],
    /// Specs of `Program::elements`, in first-use order.
    elements: Vec<usize>,
    /// The spec of the element whose direct child is checked next.
    parent: Option<usize>,
    uses: Vec<Vec<(usize, Span)>>,
    /// Inside an `if` or `for` block, where controls come and go.
    in_block: bool,
    pub(crate) records: Vec<Record>,
    pub(crate) record_index: HashMap<String, usize>,
    /// Declared events and whether a `slot` exists, per template.
    event_sigs: Vec<Vec<(String, Option<Type>)>>,
    slots: Vec<bool>,
    /// Per template, whether each instance places its slot content exactly once.
    slot_once: Vec<bool>,
    host_calls: Vec<HostCall>,
    /// File of the template being checked.
    file: usize,
    slot_used: bool,
    /// Nesting below the template root being checked.
    depth: usize,
    /// Entry ids in declaration order with the spec each names (`None` for
    /// the window), so handlers can read controls declared after them.
    pub(crate) named: Vec<(String, Option<usize>)>,
    /// Checking event statements, where named controls can be read.
    pub(crate) in_handler: bool,
}

/// Checks a program against the elements described by `specs`. `files[0]`
/// is the entry document and must have a root; other files only declare
/// components, visible by name to every file. A root `Window` is the
/// document's native window. Errors carry the index of the file they belong to.
pub fn check_program(
    files: Vec<Document>,
    specs: &[ElementSpec<'_>],
) -> Result<Program, (usize, Error)> {
    let builtin = |name: &str| name == "Window" || specs.iter().any(|spec| spec.name == name);
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
            if builtin(&record.name) || names.contains_key(&record.name) {
                return Err(error("a record cannot reuse an element or component name"));
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
            if builtin(&component.name) {
                return Err(error("a component cannot reuse an element name"));
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
    let roots: Vec<&Node> = declared.iter().map(|(_, c)| &c.root).collect();
    let mut slot_once = vec![None; declared.len() + 1];
    for template in 1..slot_once.len() {
        placed_once(template, &roots, &names, &mut slot_once);
    }
    let slot_once = slot_once
        .into_iter()
        .map(|once| once == Some(true))
        .collect();
    let root = entry.unwrap();
    let mut named = Vec::new();
    entry_ids(&root, specs, &mut named);
    let mut checker = Checker {
        names,
        signatures,
        ids: Vec::new(),
        specs,
        elements: Vec::new(),
        parent: None,
        uses: vec![Vec::new(); declared.len() + 1],
        in_block: false,
        records,
        record_index,
        event_sigs,
        slots,
        slot_once,
        host_calls: Vec::new(),
        file: 0,
        slot_used: false,
        depth: 0,
        named,
        in_handler: false,
    };
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
    debug_assert!(
        checker
            .ids
            .iter()
            .map(|(id, _)| id)
            .eq(checker.named.iter().map(|(id, _)| id)),
        "entry ids are collected in checking order"
    );
    Ok(Program {
        templates,
        elements: checker
            .elements
            .iter()
            .map(|&spec| specs[spec].name.to_owned())
            .collect(),
        ids: checker.ids,
        records: checker.records,
        host_calls: checker.host_calls,
        files: Vec::new(),
    })
}

/// Collects the ids of an entry document in the order the checker assigns
/// them: each node before its children, outside blocks, including slot
/// content. Invalid ids are reported when their node is checked.
fn entry_ids(node: &Node, specs: &[ElementSpec<'_>], ids: &mut Vec<(String, Option<usize>)>) {
    for property in &node.properties {
        if let (true, crate::Value::Identifier(id)) = (property.name == "id", &property.value) {
            let spec = specs.iter().position(|spec| spec.name == node.name);
            ids.push((id.clone(), spec));
        }
    }
    for item in &node.children {
        if let Item::Node(child) = item {
            entry_ids(child, specs, ids);
        }
    }
}

/// Whether a component body places its instance's children.
fn has_slot(node: &Node) -> bool {
    slot_in(&node.children)
}

fn slot_in(items: &[Item]) -> bool {
    items.iter().any(|item| match item {
        Item::Slot => true,
        Item::Node(node) => has_slot(node),
        Item::If(_, then, otherwise) => slot_in(then) || slot_in(otherwise),
        Item::For(_, _, _, body) => slot_in(body),
    })
}

/// Whether component `template` places its slot exactly once per instance:
/// outside `if` and `for` blocks, and through the slots of inner instances
/// only where those are placed once too. Recursive components, an error
/// reported later, count as not placing it once.
fn placed_once(
    template: usize,
    roots: &[&Node],
    names: &HashMap<String, usize>,
    memo: &mut [Option<bool>],
) -> bool {
    // Some(once) when the slot is in `node`'s subtree.
    fn find(
        node: &Node,
        roots: &[&Node],
        names: &HashMap<String, usize>,
        memo: &mut [Option<bool>],
    ) -> Option<bool> {
        node.children.iter().find_map(|item| match item {
            Item::Slot => Some(true),
            Item::Node(child) => {
                let once = find(child, roots, names, memo)?;
                Some(match names.get(&child.name) {
                    Some(&inner) => once && placed_once(inner, roots, names, memo),
                    None => once,
                })
            }
            Item::If(_, then, otherwise) => (slot_in(then) || slot_in(otherwise)).then_some(false),
            Item::For(.., body) => slot_in(body).then_some(false),
        })
    }
    if let Some(once) = memo[template] {
        return once;
    }
    memo[template] = Some(false);
    let once = find(roots[template - 1], roots, names, memo) == Some(true);
    memo[template] = Some(once);
    once
}

/// Rejects a record type that is not declared.
fn resolve(ty: &Type, records: &HashMap<String, usize>) -> Result<(), String> {
    match ty {
        Type::Record(name) if !records.contains_key(name) => Err(format!("unknown type `{name}`")),
        Type::List(item) => resolve(item, records),
        _ => Ok(()),
    }
}

impl Checker<'_> {
    fn template(
        &mut self,
        index: usize,
        name: String,
        params: Vec<Param>,
        events: Vec<crate::EventDecl>,
        mut root: Node,
    ) -> Result<Template, Error> {
        self.slot_used = false;
        self.parent = None;
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
}

/// Literals and lists of literals.
fn constant(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(_) => true,
        ExprKind::List(items) => items.iter().all(constant),
        _ => false,
    }
}
