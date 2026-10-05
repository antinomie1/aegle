//! Checked dynamic programs: states, bindings, events, blocks and components.

use std::{collections::HashMap, rc::Rc};

use crate::checked::{Bound, Child, Element, ElementKind, EventKind, Program, Step, Template};

use crate::schema::{allowed, kind as builtin, literal, property_name, valid_id, validate};
use crate::{
    Document, Error, Expr, ExprKind, Item, Kind, Node, Param, PropertyName, Span, Statement, Type,
    Value,
};

/// Names visible to expressions while checking one template.
pub(crate) struct Scope {
    pub template: usize,
    pub params: Vec<(String, Type)>,
    pub states: Vec<(String, Type)>,
    pub items: Vec<(String, Type)>,
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
}

/// Checks a program. `files[0]` is the entry document and must have a root;
/// other files only declare components, visible by name to every file.
/// Errors carry the index of the file they belong to.
pub fn check_program(files: Vec<Document>) -> Result<Program, (usize, Error)> {
    let none = Span { start: 0, end: 0 };
    let mut names = HashMap::new();
    let mut declared = Vec::new();
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
        for component in document.components {
            let error = |message: &str| (file, Error::new(component.span, message));
            if builtin(&component.name).is_some() {
                return Err(error("a component cannot reuse a built-in name"));
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
    };
    let root = entry.unwrap();
    let name = root.name.clone();
    let mut templates = vec![
        checker
            .template(0, name, Vec::new(), root)
            .map_err(|e| (0, e))?,
    ];
    let files: Vec<usize> = std::iter::once(0)
        .chain(declared.iter().map(|(f, _)| *f))
        .collect();
    for (index, (file, component)) in declared.into_iter().enumerate() {
        let template =
            checker.template(index + 1, component.name, component.params, component.root);
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
    })
}

impl Checker {
    fn template(
        &mut self,
        index: usize,
        name: String,
        params: Vec<Param>,
        mut root: Node,
    ) -> Result<Template, Error> {
        let mut scope = Scope {
            template: index,
            params: Vec::new(),
            states: Vec::new(),
            items: Vec::new(),
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
            self.expr(&mut state.value, &scope, Some(&state.ty))?;
            scope.states.push((state.name.clone(), state.ty.clone()));
            states.push((state.name, state.ty, state.value));
        }
        let root = self.element(root, &mut scope, true)?;
        Ok(Template {
            name,
            params: checked_params,
            states,
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
            children: Vec::new(),
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
                (Kind::CheckBox | Kind::Switch | Kind::Slider, "changed") => EventKind::Changed,
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

    fn instance(
        &mut self,
        node: Node,
        template: usize,
        scope: &mut Scope,
    ) -> Result<Element, Error> {
        self.uses[scope.template].push((template, node.span));
        if let Some(event) = node.events.first() {
            return Err(Error::new(event.span, "component instances have no events"));
        }
        if !node.children.is_empty() {
            return Err(Error::new(
                node.span,
                "component instances do not accept children",
            ));
        }
        let signature = self.signatures[template].clone();
        let mut arguments = vec![None; signature.len()];
        for property in node.properties {
            let index = signature
                .iter()
                .position(|(name, _, _)| *name == property.name)
                .ok_or_else(|| {
                    Error::new(
                        property.span,
                        format!("unknown parameter `{}`", property.name),
                    )
                })?;
            if arguments[index].is_some() {
                return Err(Error::new(property.span, "duplicate argument"));
            }
            let mut expr = match property.value {
                Value::Expr(expr) => *expr,
                Value::Identifier(name) => Expr {
                    kind: ExprKind::Name(name),
                    span: property.value_span,
                },
                value => Expr {
                    kind: ExprKind::Literal(value),
                    span: property.value_span,
                },
            };
            self.expr(&mut expr, scope, Some(&signature[index].1))?;
            arguments[index] = Some(Rc::new(expr));
        }
        if let Some((name, ..)) = signature
            .iter()
            .zip(&arguments)
            .find_map(|(s, a)| (a.is_none() && !s.2).then_some(s))
        {
            return Err(Error::new(node.span, format!("missing argument `{name}`")));
        }
        Ok(Element {
            kind: ElementKind::Component(template),
            id: None,
            properties: Vec::new(),
            arguments,
            events: Vec::new(),
            children: Vec::new(),
            span: node.span,
        })
    }

    fn child(&mut self, item: Item, scope: &mut Scope) -> Result<Child, Error> {
        Ok(match item {
            Item::Node(node) => Child::Element(self.element(node, scope, false)?),
            Item::If(mut condition, then, otherwise) => {
                self.expr(&mut condition, scope, Some(&Type::Bool))?;
                let then = self.block(then, scope)?;
                let otherwise = self.block(otherwise, scope)?;
                Child::If(Rc::new(condition), then, otherwise)
            }
            Item::For(name, mut list, body) => {
                let Type::List(item) = self.expr(&mut list, scope, None)? else {
                    return Err(Error::new(list.span, "for requires a list"));
                };
                scope.items.push((name, *item));
                let body = self.block(body, scope);
                scope.items.pop();
                Child::For(Rc::new(list), body?)
            }
        })
    }

    fn block(&mut self, items: Vec<Item>, scope: &mut Scope) -> Result<Rc<[Child]>, Error> {
        let previous = std::mem::replace(&mut self.in_block, true);
        let children: Result<Vec<_>, _> = items
            .into_iter()
            .map(|item| self.child(item, scope))
            .collect();
        self.in_block = previous;
        Ok(children?.into())
    }

    fn steps(&mut self, statements: Vec<Statement>, scope: &Scope) -> Result<Vec<Step>, Error> {
        let mut steps = Vec::new();
        for statement in statements {
            steps.push(match statement {
                Statement::Assign {
                    target,
                    operator,
                    mut value,
                    span,
                } => {
                    let index = scope
                        .states
                        .iter()
                        .position(|(name, _)| *name == target)
                        .ok_or_else(|| {
                            Error::new(
                                span,
                                format!("`{target}` is not a state of this document or component"),
                            )
                        })?;
                    let ty = &scope.states[index].1;
                    let numeric = matches!(ty, Type::Int | Type::Float);
                    let appendable = matches!(ty, Type::String | Type::List(_));
                    if operator != "=" && !(numeric || (operator == "+=" && appendable)) {
                        return Err(Error::new(
                            span,
                            format!("`{operator}` does not apply to this state"),
                        ));
                    }
                    self.expr(&mut value, scope, Some(ty))?;
                    Step::Assign(index, operator, value)
                }
                Statement::If(mut condition, then, otherwise) => {
                    self.expr(&mut condition, scope, Some(&Type::Bool))?;
                    Step::If(
                        condition,
                        self.steps(then, scope)?,
                        self.steps(otherwise, scope)?,
                    )
                }
            });
        }
        Ok(steps)
    }
}

/// Properties whose values may be expressions, with their types.
fn bindable(name: PropertyName) -> Option<Type> {
    use PropertyName::*;
    match name {
        Text | Label => Some(Type::String),
        Visible | Enabled | Checked | ReadOnly => Some(Type::Bool),
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
