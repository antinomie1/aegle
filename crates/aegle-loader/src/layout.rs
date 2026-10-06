//! Layout properties applied through the imperative setters.

use aegle_markup::{PropertyName, Value as Literal};
use aegle_ui::{Align, Container, Insets, Justify, Length, Node, Result};

fn length(value: &Literal) -> Length {
    match value {
        Literal::Length(v) => Length::Px(*v),
        Literal::Percent(v) => Length::Percent(*v),
        _ => Length::Auto,
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

fn identifier(value: &Literal) -> &str {
    let Literal::Identifier(name) = value else {
        unreachable!("checked identifier")
    };
    name
}

fn align(value: &Literal) -> Option<Align> {
    Some(match identifier(value) {
        "start" => Align::Start,
        "end" => Align::End,
        "center" => Align::Center,
        "stretch" => Align::Stretch,
        _ => Align::Baseline,
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
        _ => Justify::SpaceEvenly,
    })
}

fn number(value: &Literal) -> f32 {
    let Literal::Number(n) = value else {
        unreachable!("checked number")
    };
    *n
}

/// Applies a layout property, or returns `None` for other properties.
/// Container properties are only checked on containers, so `container` is
/// present whenever one of them arrives.
pub(crate) fn apply(
    node: &Node,
    container: Option<&Container>,
    name: PropertyName,
    value: &Literal,
) -> Option<Result> {
    use PropertyName::*;
    let parent = container;
    let container = || parent.expect("checked container property");
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
            _ => aegle_ui::Direction::ColumnReverse,
        }),
        LayoutDirection => node.set_layout_direction(Some(match identifier(value) {
            "ltr" => aegle_ui::LayoutDirection::Ltr,
            _ => aegle_ui::LayoutDirection::Rtl,
        })),
        Wrap => container().set_wrap(match identifier(value) {
            "no_wrap" => aegle_ui::Wrap::NoWrap,
            "wrap" => aegle_ui::Wrap::Wrap,
            _ => aegle_ui::Wrap::WrapReverse,
        }),
        Align => container().set_align_items(align(value)),
        Justify => container().set_justify_content(justify(value)),
        AlignContent => container().set_align_content(justify(value)),
        JustifySelf | JustifyItems | Columns | Rows | AutoColumns | AutoRows | Flow
        | GridColumn | GridRow => grid(node, parent, name, value),
        _ => return None,
    })
}

#[cfg(feature = "grid")]
fn grid(node: &Node, parent: Option<&Container>, name: PropertyName, value: &Literal) -> Result {
    use PropertyName::*;
    use aegle_ui::{Flow, Placement, Track};
    let container = || parent.expect("checked container property");
    let tracks = || -> Vec<Track> {
        let track = |value: &Literal| match value {
            Literal::Length(v) => Track::Px(*v),
            Literal::Percent(v) => Track::Percent(*v),
            Literal::Fraction(v) => Track::Fr(*v),
            Literal::Identifier(name) if name == "min_content" => Track::MinContent,
            Literal::Identifier(name) if name == "max_content" => Track::MaxContent,
            _ => Track::Auto,
        };
        match value {
            Literal::List(items) => items.iter().map(track).collect(),
            value => vec![track(value)],
        }
    };
    let placement = || {
        let line = |value: &Literal| match value {
            Literal::Number(n) => Some(*n as i16),
            _ => None,
        };
        match value {
            Literal::List(items) => Placement {
                line: line(&items[0]),
                span: number(&items[1]) as u16,
            },
            value => Placement {
                line: line(value),
                span: 1,
            },
        }
    };
    match name {
        JustifySelf => node.set_justify_self(align(value)),
        JustifyItems => container().set_justify_items(align(value)),
        Columns => container().set_columns(&tracks()),
        Rows => container().set_rows(&tracks()),
        AutoColumns => container().set_auto_columns(&tracks()),
        AutoRows => container().set_auto_rows(&tracks()),
        Flow => container().set_flow(match identifier(value) {
            "row" => Flow::Row,
            "column" => Flow::Column,
            "row_dense" => Flow::RowDense,
            _ => Flow::ColumnDense,
        }),
        GridColumn => node.set_grid_column(placement()),
        _ => node.set_grid_row(placement()),
    }
}

#[cfg(not(feature = "grid"))]
fn grid(_: &Node, _: Option<&Container>, name: PropertyName, _: &Literal) -> Result {
    Err(format!("markup {name:?} requires the grid feature").into())
}
