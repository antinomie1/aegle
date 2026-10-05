//! The built-in static component schema, independent of any UI runtime.

mod values;

use std::collections::HashSet;
use values::{property_name, valid_id, validate, validate_range};

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
    /// A clipped, scrollable vertical container.
    ScrollView,
    /// Static text label.
    Text,
    /// Activatable text button.
    Button,
    /// Single-line editor.
    TextField,
    /// Multiline editor.
    TextArea,
    /// A labelled two-state check box.
    CheckBox,
    /// A labelled two-state switch.
    Switch,
    /// An interactive horizontal numeric range.
    Slider,
    /// A noninteractive horizontal numeric progress indicator.
    Progress,
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
    /// Normal background color.
    Background,
    /// Normal text foreground color.
    Foreground,
    /// Outline color.
    BorderColor,
    /// Nonnegative outline thickness in logical pixels.
    BorderWidth,
    /// Nonnegative corner radius in logical pixels.
    Radius,
    /// Focus outline color for buttons, toggles, sliders and editors.
    FocusColor,
    /// Nonnegative focus outline thickness for focusable controls, in logical pixels.
    FocusWidth,
    /// Editor selection highlight color.
    SelectionColor,
    /// Editor caret color.
    CaretColor,
    /// Background color while hovered, for interactive controls.
    HoverBackground,
    /// Background color while a button, toggle or slider is pressed.
    PressedBackground,
    /// Background color while disabled.
    DisabledBackground,
    /// Text foreground color while disabled.
    DisabledForeground,
    /// Positive font size in logical pixels for text-bearing controls.
    FontSize,
    /// Whole milliseconds for subsequent appearance transitions; requires motion support.
    Transition,
    /// Transition easing; requires a sibling `transition` property.
    Easing,
    /// Initial toggle state; defaults to false.
    Checked,
    /// Finite numeric lower bound; defaults to zero.
    Min,
    /// Finite numeric upper bound, greater than min; defaults to one.
    Max,
    /// Initial numeric value, clamped by the shared constructor; defaults to zero.
    Value,
    /// Slider interval; zero selects continuous movement.
    Step,
    /// Check mark, switch thumb, slider thumb or progress fill color.
    IndicatorColor,
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
        "ScrollView" => Kind::ScrollView,
        "Text" => Kind::Text,
        "Button" => Kind::Button,
        "TextField" => Kind::TextField,
        "TextArea" => Kind::TextArea,
        "CheckBox" => Kind::CheckBox,
        "Switch" => Kind::Switch,
        "Slider" => Kind::Slider,
        "Progress" => Kind::Progress,
        _ => return Err(error(format!("unknown component `{}`", node.name))),
    };
    if kind == Kind::Window && depth != 1 {
        return Err(error("Window is only allowed at the document root".into()));
    }
    let container = matches!(
        kind,
        Kind::Window | Kind::Column | Kind::Row | Kind::ScrollView
    );
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
    validate_range(&result)?;
    if seen.contains(&PropertyName::Easing) && !seen.contains(&PropertyName::Transition) {
        let property = result
            .properties
            .iter()
            .find(|property| property.name == PropertyName::Easing)
            .unwrap();
        return Err(Error::new(
            property.span,
            "easing requires a transition duration on the same component",
        ));
    }
    for child in node.children {
        result
            .children
            .push(check_node(child, depth + 1, remaining, ids)?);
    }
    Ok(result)
}
