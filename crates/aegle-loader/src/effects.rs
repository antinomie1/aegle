//! Shadows, gradient backgrounds, group opacity and backdrop blur.

use aegle_markup::{PropertyName, Value};
use aegle_ui::{
    Color, ColorSlot, Node, Point, Result, Shadow,
    scene::{Gradient, GradientStop},
};

/// Applies a checked effect property; `None` for other properties.
pub(crate) fn apply(node: &Node, name: PropertyName, value: &Value) -> Option<Result> {
    Some(match (name, value) {
        (PropertyName::Shadow, Value::List(items)) => shadow(node, items),
        (PropertyName::BackgroundGradient, Value::Call(function, arguments)) => {
            gradient(node, function, arguments)
        }
        (PropertyName::Opacity, Value::Number(opacity)) => node.set_opacity(*opacity),
        (PropertyName::BackdropBlur, Value::Length(blur)) => node.set_backdrop_blur(*blur),
        _ => return None,
    })
}

/// A checked token call's name.
fn token(value: &Value) -> Option<&str> {
    match value {
        Value::Call(function, arguments) if function == "token" => match &arguments[..] {
            [Value::String(name)] => Some(name),
            _ => unreachable!("checked token"),
        },
        _ => None,
    }
}

/// A literal color; a token's color arrives when its binding is made.
fn color(value: &Value) -> Color {
    match *value {
        Value::Color([r, g, b, a]) => Color::rgba(r, g, b, a),
        _ => Color::TRANSPARENT,
    }
}

fn shadow(node: &Node, items: &[Value]) -> Result {
    let [
        Value::Length(x),
        Value::Length(y),
        Value::Length(blur),
        Value::Length(spread),
        paint,
    ] = items
    else {
        unreachable!("checked shadow")
    };
    node.set_shadow(Some(Shadow {
        offset: Point::new(*x, *y),
        blur: *blur,
        spread: *spread,
        color: color(paint),
    }))
}

fn gradient(node: &Node, function: &str, arguments: &[Value]) -> Result {
    let (stops, geometry) = match (function, arguments) {
        ("linear", [Value::Number(degrees), stops @ ..]) => (stops, Some(*degrees)),
        ("radial", stops) => (stops, None),
        _ => unreachable!("checked gradient"),
    };
    let last = (stops.len() - 1) as f32;
    let parts = stops.iter().zip(0..).map(|(stop, index)| match stop {
        Value::List(pair) => match &pair[..] {
            [paint, Value::Percent(percent)] => (paint, percent / 100.0),
            _ => unreachable!("checked gradient stop"),
        },
        paint => (paint, index as f32 / last),
    });
    let mut tokens = Vec::new();
    let mut resolved = [GradientStop {
        offset: 0.0,
        color: Color::TRANSPARENT,
    }; Gradient::MAX_STOPS];
    for ((paint, offset), (index, slot)) in parts.zip(resolved.iter_mut().enumerate()) {
        *slot = GradientStop {
            offset,
            color: color(paint),
        };
        if let Some(name) = token(paint) {
            tokens.push((index as u8, name));
        }
    }
    let resolved = &resolved[..stops.len()];
    let gradient = match geometry {
        // Degrees clockwise from "toward the top", measured in the unit box,
        // so 45 always runs from the bottom-left to the top-right corner.
        Some(degrees) => {
            let (sin, cos) = degrees.to_radians().sin_cos();
            let reach = (sin.abs() + cos.abs()) / 2.0;
            let (dx, dy) = (sin * reach, -cos * reach);
            let start = Point::new(0.5 - dx, 0.5 - dy);
            Gradient::linear(start, Point::new(0.5 + dx, 0.5 + dy), resolved)
        }
        None => Gradient::radial(Point::new(0.5, 0.5), 0.5, resolved),
    }
    .expect("checked gradient");
    node.set_background_gradient(Some(gradient))?;
    for (index, name) in tokens {
        node.bind_color(ColorSlot::GradientStop(index), aegle_ui::token(name)?)?;
    }
    Ok(())
}
