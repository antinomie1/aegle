//! The built-in static component schema, independent of any UI runtime.

mod values;

use std::collections::HashSet;
pub(crate) use values::{allowed, choices, property_name, valid_id, validate, validate_range};

use crate::{Document, Error, Item, Node, Span, Value};

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
    /// A grid container; requires the `grid` feature at run time.
    Grid,
    /// Children overlapping in one cell; requires the `grid` feature.
    Stack,
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
    /// A labelled choice, exclusive among its sibling radio buttons.
    RadioButton,
    /// An interactive horizontal numeric range.
    Slider,
    /// A noninteractive numeric progress indicator.
    Progress,
    /// A one-pixel divider.
    Separator,
    /// A numeric text field with steppers.
    NumberField,
    /// A tab list whose children are `Tab` pages.
    Tabs,
    /// One page of a `Tabs`, titled by `title`.
    Tab,
    /// Two panes, its two children, divided by a draggable handle.
    Splitter,
}

/// Properties shared with the imperative retained-control API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyName {
    /// Native window title.
    Title,
    /// Initial label, button or editor text.
    Text,
    /// Width: dp, percent or auto; a whole dp length on a window.
    Width,
    /// Height: dp, percent or auto; a whole dp length on a window.
    Height,
    /// Minimum width: dp, percent or auto.
    MinWidth,
    /// Minimum height: dp, percent or auto.
    MinHeight,
    /// Maximum width: dp, percent or auto (no limit).
    MaxWidth,
    /// Maximum height: dp, percent or auto (no limit).
    MaxHeight,
    /// Positive width-to-height ratio.
    AspectRatio,
    /// Inner spacing: one length or a CSS-order list of two or four.
    Padding,
    /// Outer spacing: like padding, but may be negative or auto.
    Margin,
    /// Absolute placement from the parent's edges, in CSS order; removes the item from the flow.
    Inset,
    /// Child spacing: one length, or `[row, column]` gaps.
    Gap,
    /// Nonnegative flex growth factor.
    Grow,
    /// Nonnegative flex shrink factor.
    Shrink,
    /// Main-axis size before growing and shrinking.
    Basis,
    /// Flex container main axis.
    Direction,
    /// Flex line wrapping.
    Wrap,
    /// Cross-axis alignment of children.
    Align,
    /// Distribution of main-axis free space.
    Justify,
    /// Distribution of space between lines or grid rows.
    AlignContent,
    /// This item's cross-axis alignment.
    AlignSelf,
    /// This grid item's horizontal alignment.
    JustifySelf,
    /// Grid children's horizontal alignment.
    JustifyItems,
    /// Explicit grid column tracks.
    Columns,
    /// Explicit grid row tracks.
    Rows,
    /// Implicit grid column tracks.
    AutoColumns,
    /// Implicit grid row tracks.
    AutoRows,
    /// Grid auto-placement order.
    Flow,
    /// Grid column placement: a line, or `[line or auto, span]`.
    GridColumn,
    /// Grid row placement: a line, or `[line or auto, span]`.
    GridRow,
    /// Slider, progress or splitter axis.
    Orientation,
    /// Progress of unknown length.
    Indeterminate,
    /// A hint shown after the pointer rests, also the accessible description.
    Tooltip,
    /// Digits shown after a number field's decimal point.
    Decimals,
    /// A splitter's first-pane share.
    Ratio,
    /// Visibility of the subtree.
    Visible,
    /// Whether the subtree accepts interaction.
    Enabled,
    /// Explicit accessible name.
    Label,
    /// Editor read-only state.
    ReadOnly,
    /// Single-line editor password masking.
    Password,
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
    /// Whether a check box shows the mixed (partially checked) state.
    Mixed,
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

/// Checks a static document without loading fonts or creating any UI objects.
///
/// IDs must be unique ASCII Rust identifiers other than keywords, `_` and
/// `root`. Unknown components/properties and unsupported values are errors.
/// Only containers accept children; a Window must be the document root.
/// Manually constructed ASTs are limited to 256 levels and 10,000 nodes too.
/// Documents with states, events, blocks, components or imports are rejected;
/// check those with [`crate::check_program`].
pub fn check(document: Document) -> Result<CheckedDocument, Error> {
    let span = Span { start: 0, end: 0 };
    if !document.is_static() {
        return Err(Error::new(
            span,
            "states, events, if/for blocks, components and imports require check_program",
        ));
    }
    let root = document
        .root
        .ok_or_else(|| Error::new(span, "the document has no root component"))?;
    let mut ids = HashSet::new();
    let mut remaining = 10_000;
    Ok(CheckedDocument {
        root: check_node(root, 1, &mut remaining, &mut ids)?,
    })
}

/// Resolves a built-in component name.
pub(crate) fn kind(name: &str) -> Option<Kind> {
    Some(match name {
        "Window" => Kind::Window,
        "Column" => Kind::Column,
        "Row" => Kind::Row,
        "ScrollView" => Kind::ScrollView,
        "Grid" => Kind::Grid,
        "Stack" => Kind::Stack,
        "Separator" => Kind::Separator,
        "NumberField" => Kind::NumberField,
        "Tabs" => Kind::Tabs,
        "Tab" => Kind::Tab,
        "Splitter" => Kind::Splitter,
        "Text" => Kind::Text,
        "Button" => Kind::Button,
        "TextField" => Kind::TextField,
        "TextArea" => Kind::TextArea,
        "CheckBox" => Kind::CheckBox,
        "RadioButton" => Kind::RadioButton,
        "Switch" => Kind::Switch,
        "Slider" => Kind::Slider,
        "Progress" => Kind::Progress,
        _ => return None,
    })
}

impl Kind {
    /// Whether this component accepts children.
    pub fn is_container(self) -> bool {
        matches!(
            self,
            Kind::Window
                | Kind::Column
                | Kind::Row
                | Kind::ScrollView
                | Kind::Grid
                | Kind::Stack
                | Kind::Tabs
                | Kind::Tab
                | Kind::Splitter
        )
    }
}

/// Children rules beyond containment: `Tabs` holds only `Tab` pages, a `Tab`
/// lives only in `Tabs`, and a `Splitter` holds exactly two controls. `None`
/// stands for a block, slot or component instance.
pub(crate) fn structure(parent: Kind, children: &[Option<Kind>]) -> Result<(), &'static str> {
    let tabs = children.iter().filter(|&&c| c == Some(Kind::Tab)).count();
    match parent {
        Kind::Tabs if tabs != children.len() => Err("Tabs accepts only Tab children"),
        Kind::Splitter if children.len() != 2 || children.contains(&None) => {
            Err("Splitter requires exactly two controls as children")
        }
        _ if parent != Kind::Tabs && tabs > 0 => Err("Tab is only allowed inside Tabs"),
        _ => Ok(()),
    }
}

/// Integer literals are numbers wherever a static property expects one, and
/// a list of literals and bare identifiers is a constant list value.
pub(crate) fn literal(value: Value) -> Value {
    match value {
        Value::Int(n) => Value::Number(n as f32),
        Value::Expr(expr) => constant_list(&expr).unwrap_or(Value::Expr(expr)),
        value => value,
    }
}

/// A `[...]` expression of literals and bare identifiers as a list value.
pub(crate) fn constant_list(expr: &crate::Expr) -> Option<Value> {
    let crate::ExprKind::List(items) = &expr.kind else {
        return None;
    };
    items
        .iter()
        .map(|item| match &item.kind {
            crate::ExprKind::Literal(value) => Some(literal(value.clone())),
            crate::ExprKind::Name(name) => Some(Value::Identifier(name.clone())),
            _ => None,
        })
        .collect::<Option<_>>()
        .map(Value::List)
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
    let kind =
        kind(&node.name).ok_or_else(|| error(format!("unknown component `{}`", node.name)))?;
    if kind == Kind::Window && depth != 1 {
        return Err(error("Window is only allowed at the document root".into()));
    }
    if !kind.is_container() && !node.children.is_empty() {
        return Err(error(format!("{} does not accept children", node.name)));
    }
    let kinds: Vec<_> = node
        .children
        .iter()
        .map(|item| match item {
            Item::Node(child) => crate::schema::kind(&child.name),
            _ => None,
        })
        .collect();
    structure(kind, &kinds).map_err(|message| error(message.into()))?;
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
        let value = literal(property.value);
        validate(kind, name, &value).map_err(error)?;
        result.properties.push(CheckedProperty {
            name,
            value,
            span: property.span,
        });
    }
    validate_range(
        kind,
        result.properties.iter().map(|p| (p.name, &p.value)),
        result.span,
    )?;
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
    if kind == Kind::Tab && !seen.contains(&PropertyName::Title) {
        return Err(error("Tab requires a title".into()));
    }
    for child in node.children {
        let Item::Node(child) = child else {
            unreachable!("static documents contain only nodes")
        };
        result
            .children
            .push(check_node(child, depth + 1, remaining, ids)?);
    }
    Ok(result)
}
