//! The built-in static component schema, independent of any UI runtime.

use std::collections::HashSet;

use crate::{Document, Error, Node, Span, Value};

/// A document checked against the currently implemented built-in components.
#[derive(Debug)]
pub struct CheckedDocument {
    /// The validated root and descendants.
    pub root: CheckedNode,
}

/// A component with a known kind, typed literal properties and unique ID.
#[derive(Debug)]
pub struct CheckedNode {
    /// Built-in control constructor.
    pub kind: Kind,
    /// Optional document-wide Rust field name.
    pub id: Option<String>,
    /// Validated properties in source order, excluding `id`.
    pub properties: Vec<CheckedProperty>,
    /// Validated children in source order.
    pub children: Vec<CheckedNode>,
    /// Original component source range.
    pub span: Span,
}

/// One validated literal setter or constructor argument.
#[derive(Debug)]
pub struct CheckedProperty {
    /// Known property name.
    pub name: PropertyName,
    /// Literal with the type required by this property and component.
    pub value: Value,
    /// Original property source range.
    pub span: Span,
}

/// Built-in component kinds supported by compiled static markup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Native window with a column root.
    Window,
    /// Vertical container.
    Column,
    /// Horizontal container.
    Row,
    /// Static text label.
    Text,
    /// Activatable text button.
    Button,
    /// Single-line editor.
    TextField,
    /// Multiline editor.
    TextArea,
}

/// Properties shared with the imperative retained-control API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyName {
    /// Native window title.
    Title,
    /// Initial label, button or editor text.
    Text,
    /// Logical width, or automatic sizing for non-window nodes.
    Width,
    /// Logical height, or automatic sizing for non-window nodes.
    Height,
    /// Minimum logical width.
    MinWidth,
    /// Minimum logical height.
    MinHeight,
    /// Uniform logical padding.
    Padding,
    /// Container child spacing.
    Gap,
    /// Nonnegative flex growth factor.
    Grow,
    /// Visibility of the subtree.
    Visible,
    /// Whether the subtree accepts interaction.
    Enabled,
    /// Explicit accessible name.
    Label,
    /// Editor read-only state.
    ReadOnly,
    /// Named built-in window theme.
    Theme,
}

/// Checks a parsed document without loading fonts or creating any UI objects.
///
/// IDs must be unique ASCII Rust identifiers other than keywords, `_` and
/// `root`. Unknown components/properties and unsupported values are errors.
/// Only containers accept children; a Window must be the document root.
/// Manually constructed ASTs are limited to 256 levels and 10,000 nodes too.
pub fn check(document: Document) -> Result<CheckedDocument, Error> {
    let mut ids = HashSet::new();
    let mut remaining = 10_000;
    Ok(CheckedDocument {
        root: check_node(document.root, 1, &mut remaining, &mut ids)?,
    })
}

fn check_node(
    node: Node,
    depth: usize,
    remaining: &mut usize,
    ids: &mut HashSet<String>,
) -> Result<CheckedNode, Error> {
    let error = |message| Error::new(node.span, message);
    if depth > 256 || *remaining == 0 {
        return Err(error(
            "schema limit exceeded (256 levels, 10,000 nodes)".into(),
        ));
    }
    *remaining -= 1;
    let kind = match node.name.as_str() {
        "Window" => Kind::Window,
        "Column" => Kind::Column,
        "Row" => Kind::Row,
        "Text" => Kind::Text,
        "Button" => Kind::Button,
        "TextField" => Kind::TextField,
        "TextArea" => Kind::TextArea,
        _ => return Err(error(format!("unknown component `{}`", node.name))),
    };
    if kind == Kind::Window && depth != 1 {
        return Err(error("Window is only allowed at the document root".into()));
    }
    let container = matches!(kind, Kind::Window | Kind::Column | Kind::Row);
    if !container && !node.children.is_empty() {
        return Err(error(format!("{} does not accept children", node.name)));
    }
    let mut result = CheckedNode {
        kind,
        id: None,
        properties: Vec::with_capacity(node.properties.len()),
        children: Vec::with_capacity(node.children.len()),
        span: node.span,
    };
    let mut seen = HashSet::new();
    for property in node.properties {
        let error = |message| Error::new(property.value_span, message);
        if property.name == "id" {
            if result.id.is_some() {
                return Err(error("duplicate property `id`".into()));
            }
            let Value::Identifier(id) = property.value else {
                return Err(error("id requires a Rust identifier".into()));
            };
            if !valid_id(&id) {
                return Err(error(format!("invalid or reserved id `{id}`")));
            }
            if !ids.insert(id.clone()) {
                return Err(error(format!("duplicate id `{id}`")));
            }
            result.id = Some(id);
            continue;
        }
        let name = property_name(&property.name)
            .ok_or_else(|| error(format!("unknown property `{}`", property.name)))?;
        if !seen.insert(name) {
            return Err(error(format!("duplicate property `{}`", property.name)));
        }
        validate(kind, name, &property.value).map_err(error)?;
        result.properties.push(CheckedProperty {
            name,
            value: property.value,
            span: property.span,
        });
    }
    for child in node.children {
        result
            .children
            .push(check_node(child, depth + 1, remaining, ids)?);
    }
    Ok(result)
}

fn property_name(name: &str) -> Option<PropertyName> {
    use PropertyName::*;
    Some(match name {
        "title" => Title,
        "text" => Text,
        "width" => Width,
        "height" => Height,
        "min_width" => MinWidth,
        "min_height" => MinHeight,
        "padding" => Padding,
        "gap" => Gap,
        "grow" => Grow,
        "visible" => Visible,
        "enabled" => Enabled,
        "label" => Label,
        "read_only" => ReadOnly,
        "theme" => Theme,
        _ => return None,
    })
}

fn validate(kind: Kind, name: PropertyName, value: &Value) -> Result<(), String> {
    use PropertyName::*;
    let allowed = match name {
        Title | Theme => kind == Kind::Window,
        Text => matches!(
            kind,
            Kind::Text | Kind::Button | Kind::TextField | Kind::TextArea
        ),
        ReadOnly => matches!(kind, Kind::TextField | Kind::TextArea),
        Gap => matches!(kind, Kind::Window | Kind::Column | Kind::Row),
        _ => true,
    };
    if !allowed {
        return Err(format!("{name:?} is not supported on {kind:?}"));
    }
    let valid = match (name, value) {
        (Title, Value::String(text)) => text.len() <= 4000 && !text.contains('\0'),
        (Text, Value::String(text)) if kind == Kind::TextField => !text.contains([
            '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
        ]),
        (Text | Label, Value::String(_)) => true,
        (Width | Height, Value::Length(n)) if kind == Kind::Window => {
            n.is_finite() && *n > 0.0 && n.fract() == 0.0 && f64::from(*n) <= f64::from(u32::MAX)
        }
        (Width | Height, Value::Identifier(name)) => kind != Kind::Window && name == "auto",
        (Width | Height | MinWidth | MinHeight | Padding | Gap, Value::Length(n))
        | (Grow, Value::Number(n)) => n.is_finite() && *n >= 0.0,
        (Visible | Enabled | ReadOnly, Value::Bool(_)) => true,
        (Theme, Value::Identifier(name)) => {
            matches!(name.as_str(), "light" | "dark" | "high_contrast")
        }
        _ => false,
    };
    if valid {
        return Ok(());
    }
    let expected = match name {
        Title => "a string of at most 4000 bytes without NUL",
        Text if kind == Kind::TextField => "a string without hard line separators",
        Text | Label => "a string",
        Width | Height if kind == Kind::Window => "a positive whole dp length fitting u32",
        Width | Height => "a nonnegative dp length or auto",
        MinWidth | MinHeight | Padding | Gap => "a nonnegative dp length",
        Grow => "a finite nonnegative number",
        Visible | Enabled | ReadOnly => "true or false",
        Theme => "light, dark or high_contrast",
    };
    Err(format!("{name:?} requires {expected}"))
}

fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(
            id,
            "_" | "root"
                | "as"
                | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "abstract"
                | "become"
                | "box"
                | "do"
                | "final"
                | "gen"
                | "macro"
                | "override"
                | "priv"
                | "typeof"
                | "unsized"
                | "virtual"
                | "yield"
                | "try"
        )
}
