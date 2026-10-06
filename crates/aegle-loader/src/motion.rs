//! Presented geometry and transition timing properties.

use aegle_markup::{Element, PropertyName, Value};
use aegle_ui::{Node, Point, Result, Transform};

/// Applies an offset, scale or rotation, literal or bound, keeping the other
/// parts of the node's target geometry; `None` for other properties.
pub(crate) fn geometry(node: &Node, name: PropertyName, value: &Value) -> Option<Result> {
    let (Value::Length(value) | Value::Number(value)) = *value else {
        return None;
    };
    let result = match name {
        PropertyName::OffsetX => node
            .offset()
            .and_then(|offset| node.set_offset(Point::new(value, offset.y))),
        PropertyName::OffsetY => node
            .offset()
            .and_then(|offset| node.set_offset(Point::new(offset.x, value))),
        PropertyName::Scale => node.transform().and_then(|transform| {
            node.set_transform(Transform {
                scale: value,
                ..transform
            })
        }),
        PropertyName::Rotation => node.transform().and_then(|transform| {
            node.set_transform(Transform {
                rotation: value.to_radians(),
                ..transform
            })
        }),
        _ => return None,
    };
    Some(result)
}

#[cfg(feature = "motion")]
fn easing(value: Option<&Value>) -> aegle_ui::Easing {
    use aegle_ui::Easing;
    match value {
        Some(Value::Identifier(name)) if name == "linear" => Easing::Linear,
        Some(Value::Identifier(name)) if name == "ease_in" => Easing::EaseIn,
        Some(Value::Identifier(name)) if name == "ease_in_out" => Easing::EaseInOut,
        _ => Easing::EaseOut,
    }
}

/// Installs the node-wide `transition`, then each per-property timing over it.
#[cfg(feature = "motion")]
pub(crate) fn transitions(element: &Element, handle: &crate::Handle) -> Result {
    use aegle_markup::Bound;
    use aegle_ui::{Transition, TransitionProperty as Property};
    use std::time::Duration;
    let literal = |name| {
        element
            .properties
            .iter()
            .find_map(|(n, bound)| match bound {
                Bound::Literal(value) if *n == name => Some(value),
                _ => None,
            })
    };
    let node = handle.node();
    if let Some(Value::Duration(milliseconds)) = literal(PropertyName::Transition) {
        let duration = Duration::from_millis(*milliseconds);
        let curve = easing(literal(PropertyName::Easing));
        node.set_transition(Transition::new(duration, curve))?;
    }
    for (name, bound) in &element.properties {
        let property = match name {
            PropertyName::PaintTransition => Property::Paint,
            PropertyName::OffsetTransition => Property::Offset,
            PropertyName::ScaleTransition => Property::Scale,
            PropertyName::RotationTransition => Property::Rotation,
            _ => continue,
        };
        let (milliseconds, curve) = match bound {
            Bound::Literal(Value::Duration(milliseconds)) => (*milliseconds, easing(None)),
            Bound::Literal(Value::List(items)) => match &items[..] {
                [Value::Duration(milliseconds), curve] => (*milliseconds, easing(Some(curve))),
                _ => unreachable!("checked timing"),
            },
            _ => unreachable!("checked timing"),
        };
        let timing = Transition::new(Duration::from_millis(milliseconds), curve);
        node.set_property_transition(property, Some(timing))?;
    }
    Ok(())
}

#[cfg(not(feature = "motion"))]
pub(crate) fn transitions(element: &Element, _: &crate::Handle) -> Result {
    use PropertyName::*;
    let timed = element.properties.iter().any(|(name, _)| {
        matches!(
            name,
            Transition | PaintTransition | OffsetTransition | ScaleTransition | RotationTransition
        )
    });
    if timed {
        return Err("markup transitions require the motion feature".into());
    }
    Ok(())
}
