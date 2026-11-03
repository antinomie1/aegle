//! Presented geometry, transition timing and animation properties.

use aegle_markup::{PropertyName, Value};
use aegle_ui::{Node, Point, Transform};

/// Applies an offset, scale or rotation, literal or bound, keeping the other
/// parts of the node's target geometry; `None` for other properties.
pub(crate) fn geometry(node: &Node, name: PropertyName, value: &Value) -> Option<()> {
    let (Value::Length(value) | Value::Number(value)) = *value else {
        return None;
    };
    let (offset, transform) = (node.offset(), node.transform());
    match name {
        PropertyName::OffsetX => node.set_offset(Point::new(value, offset.y)),
        PropertyName::OffsetY => node.set_offset(Point::new(offset.x, value)),
        PropertyName::Scale => node.set_transform(Transform {
            scale: value,
            ..transform
        }),
        PropertyName::Rotation => node.set_transform(Transform {
            rotation: value.to_radians(),
            ..transform
        }),
        _ => return None,
    }
    Some(())
}

/// A checked timing: a duration, `[duration, easing]` (ease-out by
/// default), or a bare spring over its own settling time.
#[cfg(feature = "motion")]
fn timing(value: &Value) -> aegle_ui::Transition {
    use aegle_ui::Transition;
    match value {
        Value::Duration(milliseconds) => {
            Transition::new(duration(*milliseconds), aegle_ui::Easing::EaseOut)
        }
        Value::List(items) => match &items[..] {
            [Value::Duration(milliseconds), curve] => {
                Transition::new(duration(*milliseconds), easing(curve))
            }
            _ => unreachable!("checked timing"),
        },
        Value::Call(_, arguments) => Transition::spring(spring(arguments)),
        _ => unreachable!("checked timing"),
    }
}

#[cfg(feature = "motion")]
fn duration(milliseconds: u64) -> std::time::Duration {
    std::time::Duration::from_millis(milliseconds)
}

/// A checked easing: a preset name, `cubic_bezier(..)` or `spring(..)`.
#[cfg(feature = "motion")]
fn easing(value: &Value) -> aegle_ui::Easing {
    use aegle_ui::Easing;
    match value {
        Value::Call(function, arguments) if function == "cubic_bezier" => {
            let [x1, y1, x2, y2] = numbers(arguments);
            Easing::cubic_bezier(x1, y1, x2, y2).expect("checked cubic_bezier")
        }
        Value::Call(_, arguments) => Easing::Spring(spring(arguments)),
        curve => match crate::layout::identifier(curve) {
            "linear" => Easing::Linear,
            "ease_in" => Easing::EaseIn,
            "ease_out" => Easing::EaseOut,
            "ease_in_out" => Easing::EaseInOut,
            other => unreachable!("checked easing `{other}`"),
        },
    }
}

#[cfg(feature = "motion")]
fn spring(arguments: &[Value]) -> aegle_ui::Spring {
    let [stiffness, damping] = numbers(arguments);
    aegle_ui::Spring::new(stiffness, damping).expect("checked spring")
}

/// The numbers of a checked call.
#[cfg(feature = "motion")]
fn numbers<const N: usize>(arguments: &[Value]) -> [f32; N] {
    std::array::from_fn(|index| match arguments[index] {
        Value::Number(number) => number,
        _ => unreachable!("checked number"),
    })
}

/// Installs a node's literal node-wide `transition`, then each per-property
/// timing over it, then starts its animations; other properties are ignored.
#[cfg(feature = "motion")]
pub fn transitions(node: &Node, properties: &[(PropertyName, &Value)]) {
    use aegle_ui::{Animate, TransitionProperty as Property};
    if let Some((_, value)) = properties
        .iter()
        .find(|(name, _)| *name == PropertyName::Transition)
    {
        node.set_transition(timing(value));
    }
    for (name, value) in properties {
        let property = match name {
            PropertyName::PaintTransition => Property::Paint,
            PropertyName::OffsetTransition => Property::Offset,
            PropertyName::ScaleTransition => Property::Scale,
            PropertyName::RotationTransition => Property::Rotation,
            PropertyName::ShadowTransition => Property::Shadow,
            PropertyName::OpacityTransition => Property::Opacity,
            PropertyName::ScaleAnimation => {
                node.animate(Animate::Scale(animation(value, 1.0)));
                continue;
            }
            PropertyName::RotationAnimation => {
                node.animate(Animate::Rotation(animation(value, 1f32.to_radians())));
                continue;
            }
            PropertyName::OpacityAnimation => {
                node.animate(Animate::Opacity(animation(value, 1.0)));
                continue;
            }
            _ => continue,
        };
        node.set_property_transition(property, timing(value));
    }
}

/// A checked `[[values...], duration, easing?, repeat?]` with its values
/// spread evenly over the cycle and multiplied by `unit`.
#[cfg(feature = "motion")]
fn animation(value: &Value, unit: f32) -> aegle_ui::Animation<f32> {
    use aegle_ui::{Animation, Cycles, Easing, Keyframe};
    let Value::List(items) = value else {
        unreachable!("checked animation")
    };
    let [
        Value::List(values),
        Value::Duration(milliseconds),
        rest @ ..,
    ] = &items[..]
    else {
        unreachable!("checked animation")
    };
    let repeat = |v: &Value| matches!(v, Value::Identifier(name) if name == "forever" || name == "alternate");
    let (curve, repeat) = match rest {
        [] => (Easing::Linear, None),
        [last] if repeat(last) => (Easing::Linear, Some(last)),
        [curve] => (easing(curve), None),
        [curve, last] => (easing(curve), Some(last)),
        _ => unreachable!("checked animation"),
    };
    let last = (values.len() - 1) as f32;
    let frames: Vec<_> = (values.iter().zip(0..))
        .map(|(value, index)| {
            let Value::Number(value) = value else {
                unreachable!("checked animation value")
            };
            Keyframe::new(index as f32 / last, value * unit).easing(curve)
        })
        .collect();
    let animation = Animation::new(duration(*milliseconds), frames).expect("checked animation");
    match repeat.map(crate::layout::identifier) {
        None => animation,
        Some(name) => animation
            .cycles(Cycles::Forever)
            .alternate(name == "alternate"),
    }
}

/// Installs timing properties; they require the `motion` feature.
#[cfg(not(feature = "motion"))]
pub fn transitions(_: &Node, properties: &[(PropertyName, &Value)]) {
    let timed = properties.iter().any(|(name, _)| name.timed());
    assert!(
        !timed,
        "markup transitions and animations require the motion feature"
    );
}
