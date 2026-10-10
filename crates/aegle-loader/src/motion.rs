//! Presented geometry and transition timing properties.

use aegle_markup::{PropertyName, Value};
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

/// A checked timing: a duration, or `[duration, easing]`; ease-out by default.
#[cfg(feature = "motion")]
fn timing(value: &Value) -> aegle_ui::Transition {
    use aegle_ui::Easing;
    let (milliseconds, curve) = match value {
        Value::Duration(milliseconds) => (*milliseconds, Easing::EaseOut),
        Value::List(items) => match &items[..] {
            [Value::Duration(milliseconds), curve] => {
                let curve = match crate::layout::identifier(curve) {
                    "linear" => Easing::Linear,
                    "ease_in" => Easing::EaseIn,
                    "ease_out" => Easing::EaseOut,
                    "ease_in_out" => Easing::EaseInOut,
                    other => unreachable!("checked easing `{other}`"),
                };
                (*milliseconds, curve)
            }
            _ => unreachable!("checked timing"),
        },
        _ => unreachable!("checked timing"),
    };
    aegle_ui::Transition::new(std::time::Duration::from_millis(milliseconds), curve)
}

/// Installs a node's literal node-wide `transition`, then each per-property
/// timing over it; other properties are ignored.
#[cfg(feature = "motion")]
pub fn transitions(node: &Node, properties: &[(PropertyName, &Value)]) -> Result {
    use aegle_ui::TransitionProperty as Property;
    if let Some((_, value)) = properties
        .iter()
        .find(|(name, _)| *name == PropertyName::Transition)
    {
        node.set_transition(timing(value))?;
    }
    for (name, value) in properties {
        let property = match name {
            PropertyName::PaintTransition => Property::Paint,
            PropertyName::OffsetTransition => Property::Offset,
            PropertyName::ScaleTransition => Property::Scale,
            PropertyName::RotationTransition => Property::Rotation,
            PropertyName::ShadowTransition => Property::Shadow,
            PropertyName::OpacityTransition => Property::Opacity,
            _ => continue,
        };
        node.set_property_transition(property, timing(value))?;
    }
    Ok(())
}

/// Installs timing properties; they require the `motion` feature.
#[cfg(not(feature = "motion"))]
pub fn transitions(_: &Node, properties: &[(PropertyName, &Value)]) -> Result {
    use PropertyName::*;
    let timed = properties.iter().any(|(name, _)| {
        matches!(
            name,
            Transition
                | PaintTransition
                | OffsetTransition
                | ScaleTransition
                | RotationTransition
                | ShadowTransition
                | OpacityTransition
        )
    });
    if timed {
        return Err("markup transitions require the motion feature".into());
    }
    Ok(())
}
