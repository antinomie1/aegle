//! Layout properties applied through the imperative setters.

#[cfg(feature = "grid")]
mod grid;

use aegle_markup::{PropertyName, Value as Literal};
use aegle_ui::{Align, Container, Insets, Justify, Length, Node, Result};

fn length(value: &Literal) -> Length {
    match value {
        Literal::Length(v) => Length::Px(*v),
        Literal::Percent(v) => Length::Percent(*v),
        Literal::Identifier(name) if name == "auto" => Length::Auto,
        Literal::Call(_, parts) => match parts[..] {
            [Literal::Percent(percent), Literal::Length(px)] => Length::Calc { percent, px },
            _ => unreachable!("checked calc"),
        },
        _ => unreachable!("checked length"),
    }
}

/// One length, `[vertical, horizontal]` or `[top, right, bottom, left]`.
fn insets(value: &Literal) -> Insets {
    match value {
        Literal::List(items) if items.len() == 2 => {
            Insets::symmetric(length(&items[1]), length(&items[0]))
        }
        Literal::List(items) => Insets::new(
            length(&items[0]),
            length(&items[1]),
            length(&items[2]),
            length(&items[3]),
        ),
        value => Insets::all(length(value)),
    }
}

pub(crate) fn identifier(value: &Literal) -> &str {
    let Literal::Identifier(name) = value else {
        unreachable!("checked identifier")
    };
    name
}

pub(crate) fn align(value: &Literal) -> Option<Align> {
    Some(match identifier(value) {
        "start" => Align::Start,
        "end" => Align::End,
        "center" => Align::Center,
        "stretch" => Align::Stretch,
        "baseline" => Align::Baseline,
        other => unreachable!("checked alignment `{other}`"),
    })
}

fn justify(value: &Literal) -> Option<Justify> {
    Some(match identifier(value) {
        "start" => Justify::Start,
        "end" => Justify::End,
        "center" => Justify::Center,
        "stretch" => Justify::Stretch,
        "space_between" => Justify::SpaceBetween,
        "space_around" => Justify::SpaceAround,
        "space_evenly" => Justify::SpaceEvenly,
        other => unreachable!("checked justification `{other}`"),
    })
}

pub(crate) fn number(value: &Literal) -> f32 {
    let Literal::Number(n) = value else {
        unreachable!("checked number")
    };
    *n
}

/// Applies a layout property, or returns `None` for other properties.
/// Container properties are only checked on containers.
pub(crate) fn apply(node: &Node, name: PropertyName, value: &Literal) -> Option<Result> {
    use PropertyName::*;
    let container = || Container(node.clone());
    Some(match name {
        Width => node.set_width(length(value)),
        Height => node.set_height(length(value)),
        MinWidth => node.set_min_width(length(value)),
        MinHeight => node.set_min_height(length(value)),
        MaxWidth => node.set_max_width(length(value)),
        MaxHeight => node.set_max_height(length(value)),
        Basis => node.set_basis(length(value)),
        AspectRatio => node.set_aspect_ratio(Some(number(value))),
        Shrink => node.set_shrink(number(value)),
        Padding => node.set_padding(insets(value)),
        Margin => node.set_margin(insets(value)),
        Inset => node.set_absolute(Some(insets(value))),
        Gap => match value {
            Literal::List(items) => node.set_gaps(length(&items[1]), length(&items[0])),
            value => node.set_gap(length(value)),
        },
        AlignSelf => node.set_align_self(align(value)),
        Direction => container().set_direction(match identifier(value) {
            "row" => aegle_ui::Direction::Row,
            "column" => aegle_ui::Direction::Column,
            "row_reverse" => aegle_ui::Direction::RowReverse,
            "column_reverse" => aegle_ui::Direction::ColumnReverse,
            other => unreachable!("checked direction `{other}`"),
        }),
        LayoutDirection => node.set_layout_direction(Some(match identifier(value) {
            "ltr" => aegle_ui::LayoutDirection::Ltr,
            "rtl" => aegle_ui::LayoutDirection::Rtl,
            other => unreachable!("checked layout direction `{other}`"),
        })),
        Wrap => container().set_wrap(match identifier(value) {
            "no_wrap" => aegle_ui::Wrap::NoWrap,
            "wrap" => aegle_ui::Wrap::Wrap,
            "wrap_reverse" => aegle_ui::Wrap::WrapReverse,
            other => unreachable!("checked wrap `{other}`"),
        }),
        Align => container().set_align_items(align(value)),
        Justify => container().set_justify_content(justify(value)),
        AlignContent => container().set_align_content(justify(value)),
        JustifySelf | JustifyItems | Columns | Rows | AutoColumns | AutoRows | Flow
        | GridColumn | GridRow | Areas | GridArea => grid::apply(node, name, value),
        _ => return None,
    })
}

#[cfg(not(feature = "grid"))]
mod grid {
    use super::*;

    pub(crate) fn apply(_: &Node, name: PropertyName, _: &Literal) -> Result {
        Err(format!("markup {name:?} requires the grid feature").into())
    }
}
