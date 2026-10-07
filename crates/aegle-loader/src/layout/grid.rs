//! Grid tracks, templates, areas and placements.

use aegle_markup::{PropertyName, Value as Literal};
use aegle_ui::{Container, GridLine, GridLines, Node, Repeat, Result, TemplateItem, Track};

use super::{align, identifier};

fn items(value: &Literal) -> &[Literal] {
    match value {
        Literal::List(items) => items,
        value => std::slice::from_ref(value),
    }
}

fn track(value: &Literal) -> Track {
    match value {
        Literal::Length(v) => Track::Px(*v),
        Literal::Percent(v) => Track::Percent(*v),
        Literal::Fraction(v) => Track::Fr(*v),
        Literal::Identifier(name) => match name.as_str() {
            "auto" => Track::Auto,
            "min_content" => Track::MinContent,
            "max_content" => Track::MaxContent,
            other => unreachable!("checked track `{other}`"),
        },
        Literal::Call(_, arguments) => match arguments[..] {
            [Literal::Length(min), Literal::Fraction(fr)] => Track::MinMax(min, fr),
            [Literal::Length(max)] => Track::FitContent(max),
            _ => unreachable!("checked track function"),
        },
        _ => unreachable!("checked track"),
    }
}

fn template_item(value: &Literal) -> TemplateItem {
    match value {
        Literal::String(name) => TemplateItem::Line(name.clone()),
        Literal::Call(function, arguments) if function == "repeat" => {
            let count = match &arguments[0] {
                Literal::Number(n) => Repeat::Count(*n as u16),
                Literal::Identifier(name) if name == "auto_fill" => Repeat::AutoFill,
                Literal::Identifier(name) if name == "auto_fit" => Repeat::AutoFit,
                _ => unreachable!("checked repeat count"),
            };
            TemplateItem::Repeat(count, arguments[1..].iter().map(template_item).collect())
        }
        value => TemplateItem::Track(track(value)),
    }
}

fn grid_line(value: &Literal, end: bool) -> GridLine {
    match value {
        Literal::Number(n) if end => GridLine::Span(*n as u16),
        Literal::Number(n) => GridLine::Line(*n as i16),
        Literal::String(name) => GridLine::Named(name.clone(), 1),
        Literal::Identifier(name) if name == "auto" => GridLine::Auto,
        _ => unreachable!("checked grid line"),
    }
}

/// A line or name alone covers one track or the named area; a pair is
/// `[start, span or end name]`.
fn placement(value: &Literal) -> GridLines {
    match value {
        Literal::List(items) => GridLines {
            start: grid_line(&items[0], false),
            end: grid_line(&items[1], true),
        },
        Literal::String(name) => GridLines::named(name.as_str()),
        value => GridLines {
            start: grid_line(value, false),
            end: GridLine::Span(1),
        },
    }
}

pub(crate) fn apply(
    node: &Node,
    parent: Option<&Container>,
    name: PropertyName,
    value: &Literal,
) -> Result {
    use PropertyName::*;
    let container = || parent.expect("checked container property");
    let template = || items(value).iter().map(template_item).collect::<Vec<_>>();
    let tracks = || items(value).iter().map(track).collect::<Vec<_>>();
    match name {
        JustifySelf => node.set_justify_self(align(value)),
        JustifyItems => container().set_justify_items(align(value)),
        Columns => container().set_column_template(&template()),
        Rows => container().set_row_template(&template()),
        AutoColumns => container().set_auto_columns(&tracks()),
        AutoRows => container().set_auto_rows(&tracks()),
        Flow => container().set_flow(match identifier(value) {
            "row" => aegle_ui::Flow::Row,
            "column" => aegle_ui::Flow::Column,
            "row_dense" => aegle_ui::Flow::RowDense,
            "column_dense" => aegle_ui::Flow::ColumnDense,
            other => unreachable!("checked flow `{other}`"),
        }),
        Areas => {
            let rows: Vec<&str> = items(value)
                .iter()
                .map(|row| match row {
                    Literal::String(row) => row.as_str(),
                    _ => unreachable!("checked area rows"),
                })
                .collect();
            container().set_areas(&rows)
        }
        GridArea => match value {
            Literal::String(name) => node.set_grid_area(name),
            _ => unreachable!("checked area name"),
        },
        GridColumn => node.set_grid_column(placement(value)),
        GridRow => node.set_grid_row(placement(value)),
        _ => unreachable!("dispatched grid property"),
    }
}
