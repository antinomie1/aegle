//! Component instances, blocks and structural children.

use std::rc::Rc;

use super::{Checker, Scope};
use crate::checked::{Child, Element, ElementKind, Handler};
use crate::{Error, Expr, ExprKind, Item, Limits, Node, Span, Type, Value};

impl Checker {
    pub(super) fn instance(
        &mut self,
        node: Node,
        template: usize,
        scope: &mut Scope,
    ) -> Result<Element, Error> {
        self.uses[scope.template].push((template, node.span));
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
        let mut handlers: Vec<Handler> = Vec::new();
        for event in node.events {
            let sigs = &self.event_sigs[template];
            let index = sigs
                .iter()
                .position(|(name, _)| *name == event.name)
                .ok_or_else(|| {
                    Error::new(
                        event.span,
                        format!("component has no event `{}`", event.name),
                    )
                })?;
            if handlers.iter().any(|h| h.event == index) {
                return Err(Error::new(event.span, "duplicate event handler"));
            }
            let value = sigs[index].1.clone();
            let binds_value = match (&event.param, value) {
                (Some(name), Some(ty)) => {
                    scope.locals.push((name.clone(), ty));
                    true
                }
                (Some(_), None) => {
                    return Err(Error::new(event.span, "this event carries no value"));
                }
                (None, _) => false,
            };
            let steps = self.steps(event.body, scope);
            if binds_value {
                scope.locals.pop();
            }
            handlers.push(Handler {
                event: index,
                binds_value,
                steps: steps?.into(),
            });
        }
        let slot = if node.children.is_empty() {
            None
        } else if self.slots[template] {
            Some(self.block(node.children, scope)?)
        } else {
            return Err(Error::new(
                node.span,
                "this component has no slot for children",
            ));
        };
        Ok(Element {
            kind: ElementKind::Component(template),
            id: None,
            properties: Vec::new(),
            arguments,
            events: Vec::new(),
            handlers,
            children: Vec::new(),
            slot,
            span: node.span,
        })
    }

    /// Checks one nested item, rejecting hand-built documents nested deeper
    /// than any parse allows.
    pub(super) fn child(&mut self, item: Item, scope: &mut Scope) -> Result<Child, Error> {
        if self.depth + 1 == Limits::MAX_DEPTH {
            let span = match &item {
                Item::Node(node) => node.span,
                Item::If(condition, ..) => condition.span,
                Item::For(_, list, ..) => list.span,
                Item::Slot => Span::default(),
            };
            return Err(Error::new(span, crate::schema::TOO_DEEP));
        }
        self.depth += 1;
        let child = self.nested(item, scope);
        self.depth -= 1;
        child
    }

    fn nested(&mut self, item: Item, scope: &mut Scope) -> Result<Child, Error> {
        Ok(match item {
            Item::Node(node) => Child::Element(self.element(node, scope, false)?),
            Item::If(mut condition, then, otherwise) => {
                self.expr(&mut condition, scope, Some(&Type::Bool))?;
                let then = self.block(then, scope)?;
                let otherwise = self.block(otherwise, scope)?;
                Child::If(Rc::new(condition), then, otherwise)
            }
            Item::For(name, mut list, key, body) => {
                let Type::List(item) = self.expr(&mut list, scope, None)? else {
                    return Err(Error::new(list.span, "for requires a list"));
                };
                scope.items.push((name, *item.clone()));
                let key = match key {
                    Some(mut key) => {
                        let ty = self.expr(&mut key, scope, None);
                        if !matches!(ty, Ok(Type::Int | Type::String)) {
                            scope.items.pop();
                            return Err(match ty {
                                Err(error) => error,
                                Ok(_) => Error::new(key.span, "a for key must be an int or string"),
                            });
                        }
                        Some(Rc::new(key))
                    }
                    None if !matches!(*item, Type::Int | Type::String) => {
                        scope.items.pop();
                        return Err(Error::new(
                            list.span,
                            "only int or string items are their own key; add `key` followed by an int or string expression",
                        ));
                    }
                    None => None,
                };
                let body = self.block(body, scope);
                scope.items.pop();
                Child::For(Rc::new(list), key, body?)
            }
            Item::Slot => {
                if scope.template == 0 || self.slot_used {
                    return Err(Error::new(
                        Span { start: 0, end: 0 },
                        "`slot` belongs once in a component body",
                    ));
                }
                self.slot_used = true;
                Child::Slot
            }
        })
    }

    pub(super) fn block(
        &mut self,
        items: Vec<Item>,
        scope: &mut Scope,
    ) -> Result<Rc<[Child]>, Error> {
        let previous = std::mem::replace(&mut self.in_block, true);
        let children: Result<Vec<_>, _> = items
            .into_iter()
            .map(|item| self.child(item, scope))
            .collect();
        self.in_block = previous;
        Ok(children?.into())
    }
}
