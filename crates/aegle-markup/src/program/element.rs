//! Window and element nodes: properties, events and children checked
//! against the element's spec.

use std::rc::Rc;

use super::{Checker, Scope};
use crate::checked::{Bound, Element, ElementKind, Prop};
use crate::schema::{Target, allowed, choices, literal, property_name, valid_id, validate};
use crate::{Children, Error, Expr, ExprKind, Item, Node, PropertyName, Type, Value, ValueType};

impl Checker<'_> {
    /// Checks a node; `top` marks the template root.
    pub(super) fn element(
        &mut self,
        node: Node,
        scope: &mut Scope,
        top: bool,
    ) -> Result<Element, Error> {
        let parent = self.parent.take();
        if let Some(state) = node.states.first() {
            return Err(Error::new(
                state.span,
                "states belong on the root of a document or component",
            ));
        }
        if let Some(&template) = self.names.get(&node.name) {
            return self.instance(node, template, scope);
        }
        let error = |message: String| Error::new(node.span, message);
        let (kind, spec) = if node.name == "Window" {
            if !(top && scope.template == 0) {
                return Err(error("Window is only allowed at the document root".into()));
            }
            (ElementKind::Window, None)
        } else {
            let spec = self
                .specs
                .iter()
                .position(|spec| spec.name == node.name)
                .ok_or_else(|| error(format!("unknown component `{}`", node.name)))?;
            let index = match self.elements.iter().position(|&used| used == spec) {
                Some(index) => index,
                None => {
                    self.elements.push(spec);
                    self.elements.len() - 1
                }
            };
            (ElementKind::Control(index), Some(spec))
        };
        let specs = self.specs;
        let target = spec.map_or(Target::Window, |spec| Target::Element(&specs[spec]));
        if let Some(required) = spec.and_then(|spec| specs[spec].parent)
            && parent.map(|parent| specs[parent].name) != Some(required)
        {
            return Err(error(format!(
                "{} is only allowed inside {required}",
                node.name
            )));
        }
        self.children_fit(&node, target)?;
        let mut element = Element {
            kind,
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
                // Views expose entry ids and states as fields of one struct.
                if scope.states.iter().any(|(state, _)| *state == id) {
                    return Err(error(format!("`{id}` names both a control and a state")));
                }
                element.id = Some(self.ids.len());
                self.ids.push((id, kind));
                continue;
            }
            let element_property = || match target {
                Target::Element(spec) => spec.property(&property.name),
                Target::Window => None,
            };
            // An element's own property hides a node property of that name.
            let prop = match (element_property(), property_name(&property.name)) {
                (Some((index, _)), _) => Prop::Element(index),
                (None, Some(name)) => Prop::Node(name),
                (None, None) => return Err(error(format!("unknown property `{}`", property.name))),
            };
            if element.properties.iter().any(|(p, _)| *p == prop) {
                return Err(error(format!("duplicate property `{}`", property.name)));
            }
            let identifier = match prop {
                Prop::Node(name) => identifier_literal(name),
                Prop::Element(_) => {
                    matches!(element_property().unwrap().1.ty, ValueType::Choice(_))
                }
            };
            let value = match literal(property.value) {
                Value::Identifier(id) if !identifier => Err(Expr {
                    kind: ExprKind::Name(id),
                    span: property.value_span,
                }),
                Value::Expr(expr) => Err(*expr),
                value => Ok(value),
            };
            let value = match (value, prop) {
                (Ok(value), Prop::Node(name)) => {
                    validate(target, name, &value).map_err(error)?;
                    Bound::Literal(value)
                }
                (Ok(value), Prop::Element(_)) => {
                    let ty = element_property().unwrap().1.ty;
                    ty.validate(&value).map_err(|expected| {
                        error(format!("`{}` requires {expected}", property.name))
                    })?;
                    Bound::Literal(value)
                }
                (Err(mut expr), prop) => {
                    let ty = match prop {
                        Prop::Node(name) => bindable(name).filter(|_| allowed(target, name)),
                        Prop::Element(_) => {
                            let spec = element_property().unwrap().1;
                            spec.ty.binding().filter(|_| spec.set && !spec.required)
                        }
                    };
                    let ty = ty.ok_or_else(|| {
                        error(format!(
                            "`{}` on {} accepts only literal values",
                            property.name,
                            target.name()
                        ))
                    })?;
                    self.expr(&mut expr, scope, Some(&ty))?;
                    Bound::Expr(Rc::new(expr))
                }
            };
            element.properties.push((prop, value));
        }
        let has = |prop| element.properties.iter().any(|(p, _)| *p == prop);
        if let Target::Element(spec) = target
            && let Some(missing) = (0..spec.properties.len())
                .find(|&index| spec.properties[index].required && !has(Prop::Element(index)))
        {
            let name = spec.properties[missing].name;
            return Err(error(format!("{} requires `{name}`", spec.name)));
        }
        for event in node.events {
            let index = match target {
                Target::Element(spec) => spec.events.iter().position(|e| *e == event.name),
                Target::Window => None,
            }
            .ok_or_else(|| {
                Error::new(
                    event.span,
                    format!("{} has no event `{}`", target.name(), event.name),
                )
            })?;
            if element.events.iter().any(|(e, _)| *e == index) {
                return Err(Error::new(event.span, "duplicate event handler"));
            }
            if event.param.is_some() {
                return Err(Error::new(event.span, "element events carry no value"));
            }
            scope.source = spec;
            let steps = self.steps(event.body, scope);
            scope.source = None;
            element.events.push((index, steps?.into()));
        }
        for item in node.children {
            self.parent = spec;
            element.children.push(self.child(item, scope)?);
        }
        Ok(element)
    }

    /// Checks a node's children against its target's children rule.
    fn children_fit(&self, node: &Node, target: Target<'_>) -> Result<(), Error> {
        let error = |message: String| Err(Error::new(node.span, message));
        let Target::Element(spec) = target else {
            return Ok(());
        };
        if !spec.is_container() {
            if !node.children.is_empty() {
                return error(format!("{} does not accept children", spec.name));
            }
            return Ok(());
        }
        let direct = |name: Option<&str>| {
            node.children.iter().all(|item| match item {
                Item::Node(child) => {
                    !self.names.contains_key(&child.name) && name.is_none_or(|n| child.name == n)
                }
                _ => false,
            })
        };
        match spec.children {
            Children::Any => Ok(()),
            Children::Only(name) if !direct(Some(name)) => {
                error(format!("{} accepts only {name} children", spec.name))
            }
            Children::Exactly(count) if node.children.len() != count.into() || !direct(None) => {
                error(format!(
                    "{} requires exactly {count} controls as children",
                    spec.name
                ))
            }
            _ => Ok(()),
        }
    }
}

/// Node properties whose values may be expressions, with their types.
fn bindable(name: PropertyName) -> Option<Type> {
    use PropertyName::*;
    match name {
        Label | Tooltip => Some(Type::String),
        Visible | Enabled => Some(Type::Bool),
        OffsetX | OffsetY | Scale | Rotation | Opacity => Some(Type::Float),
        _ => None,
    }
}

/// Node properties that take bare identifiers as enum values or `auto`.
fn identifier_literal(name: PropertyName) -> bool {
    use PropertyName::*;
    !choices(name).is_empty()
        || matches!(
            name,
            Width
                | Height
                | MinWidth
                | MinHeight
                | MaxWidth
                | MaxHeight
                | Basis
                | Margin
                | Inset
                | Columns
                | Rows
                | AutoColumns
                | AutoRows
        )
}
