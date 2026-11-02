//! A Window document's native window: its title, size and, with `layer`, the
//! desktop-shell layer it opens as and keeps across reloads.

use aegle_app::{
    Anchor, App, KeyboardInteractivity, Layer, LayerOptions, OutputId, Window, WindowOptions,
};
use aegle_markup::{Bound, Element, Prop, PropertyName, Value};
use aegle_ui::Result;

/// What a reload must keep: whether the window is a layer, and its output.
pub(crate) struct Surface {
    layer: bool,
    /// The output named in the document, as resolved when the window opened.
    name: Option<String>,
    output: Option<OutputId>,
    /// The output was chosen by the host and overrides the document's.
    pinned: bool,
}

fn literal(root: &Element, name: PropertyName) -> Option<&Value> {
    root.properties
        .iter()
        .find_map(|(prop, bound)| match bound {
            Bound::Literal(value) if *prop == Prop::Node(name) => Some(value),
            _ => None,
        })
}

/// The checked whole dp of a length literal.
fn whole(value: &Value) -> i32 {
    let Value::Length(n) = value else {
        unreachable!("checked whole dp length")
    };
    *n as i32
}

/// The document's size, and its layer with every property but the output.
fn placement(root: &Element) -> (u32, u32, Option<LayerOptions>) {
    let defaults = WindowOptions::default();
    let size = |name, default| match literal(root, name) {
        Some(Value::Length(n)) => *n as u32,
        _ => default,
    };
    let width = size(PropertyName::Width, defaults.width);
    let height = size(PropertyName::Height, defaults.height);
    let Some(Value::Identifier(layer)) = literal(root, PropertyName::Layer) else {
        return (width, height, None);
    };
    let layer = match layer.as_str() {
        "background" => Layer::Background,
        "bottom" => Layer::Bottom,
        "top" => Layer::Top,
        "overlay" => Layer::Overlay,
        other => unreachable!("checked layer `{other}`"),
    };
    let edge = |value: &Value| match value {
        Value::Identifier(edge) => match edge.as_str() {
            "top" => Anchor::TOP,
            "bottom" => Anchor::BOTTOM,
            "left" => Anchor::LEFT,
            "right" => Anchor::RIGHT,
            other => unreachable!("checked edge `{other}`"),
        },
        _ => unreachable!("checked anchor"),
    };
    let anchor = match literal(root, PropertyName::Anchor) {
        Some(Value::List(edges)) => edges.iter().map(edge).fold(Anchor::NONE, |a, e| a | e),
        Some(value) => edge(value),
        None => Anchor::NONE,
    };
    let exclusive_zone = match literal(root, PropertyName::ExclusiveZone) {
        Some(Value::Identifier(_)) => -1,
        Some(value) => whole(value),
        None => 0,
    };
    let keyboard = match literal(root, PropertyName::Keyboard) {
        Some(Value::Identifier(policy)) => match policy.as_str() {
            "none" => KeyboardInteractivity::None,
            "exclusive" => KeyboardInteractivity::Exclusive,
            "on_demand" => KeyboardInteractivity::OnDemand,
            other => unreachable!("checked keyboard `{other}`"),
        },
        _ => KeyboardInteractivity::None,
    };
    // CSS order: one value for every edge, two for vertical and horizontal.
    let margin = match literal(root, PropertyName::Margin) {
        Some(Value::List(edges)) => match &edges[..] {
            [vertical, horizontal] => {
                let (v, h) = (whole(vertical), whole(horizontal));
                [v, h, v, h]
            }
            [top, right, bottom, left] => [whole(top), whole(right), whole(bottom), whole(left)],
            _ => unreachable!("checked margin"),
        },
        Some(value) => [whole(value); 4],
        None => [0; 4],
    };
    let layer = LayerOptions {
        layer,
        anchor,
        exclusive_zone,
        keyboard,
        margin,
        output: None,
    };
    (width, height, Some(layer))
}

fn output_name(root: &Element) -> Option<String> {
    match literal(root, PropertyName::Output) {
        Some(Value::String(name)) => Some(name.clone()),
        _ => None,
    }
}

/// Opens the native window of a Window document root, as a layer on its
/// named output or on `pinned` when the document has `layer`.
pub(crate) fn open(
    app: &App,
    root: &Element,
    pinned: Option<OutputId>,
) -> Result<(Window, Surface)> {
    let title = match literal(root, PropertyName::Title) {
        Some(Value::String(title)) => title.as_str(),
        _ => "Aegle",
    };
    let (width, height, mut layer) = placement(root);
    let name = output_name(root);
    let output = match (pinned, &name) {
        (Some(output), _) => Some(output),
        (None, Some(name)) if layer.is_some() => Some(app.output(name)?.id),
        _ => None,
    };
    if let Some(layer) = &mut layer {
        layer.output = output;
    }
    let options = WindowOptions {
        width,
        height,
        layer,
        ..Default::default()
    };
    let surface = Surface {
        layer: layer.is_some(),
        name,
        output,
        pinned: pinned.is_some(),
    };
    Ok((app.window_with_options(title, options)?, surface))
}

impl Surface {
    /// Checks that a reloaded root keeps this window's role and output, and
    /// returns the layer placement to apply once its content is built.
    pub(crate) fn reload(&self, root: &Element) -> Result<Option<(LayerOptions, u32, u32)>> {
        let (width, height, layer) = placement(root);
        let Some(mut layer) = layer else {
            if self.layer {
                return Err("a reload cannot turn a layer back into a window".into());
            }
            return Ok(None);
        };
        if !self.layer {
            return Err("a reload cannot turn a window into a layer; reopen it".into());
        }
        if !self.pinned && output_name(root) != self.name {
            return Err("a reload cannot move a layer to another output; reopen it".into());
        }
        layer.output = self.output;
        Ok(Some((layer, width, height)))
    }
}
