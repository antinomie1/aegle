//! Grid values: track lists with line names and functions, named areas and
//! placements by number or name. The rules match `aegle-layout`'s, so a
//! checked document never fails them at run time.

use crate::Value as Literal;

fn size(n: f32) -> bool {
    n.is_finite() && n >= 0.0
}

/// A line or area name: a nonempty string.
pub(super) fn name(value: &Literal) -> bool {
    matches!(value, Literal::String(name) if !name.is_empty())
}

/// One track, `Some(fixed)` when valid: `dp` and `%` are fixed, as is
/// `minmax(dp, fr)`; `fr`, `auto`, `min_content`, `max_content` and
/// `fit_content(dp)` are not.
fn track(value: &Literal) -> Option<bool> {
    match value {
        Literal::Length(n) | Literal::Percent(n) => size(*n).then_some(true),
        Literal::Fraction(n) => size(*n).then_some(false),
        Literal::Identifier(name) => {
            matches!(name.as_str(), "auto" | "min_content" | "max_content").then_some(false)
        }
        Literal::Call(function, arguments) => match (function.as_str(), &arguments[..]) {
            ("minmax", [Literal::Length(min), Literal::Fraction(fr)]) => {
                (size(*min) && size(*fr)).then_some(true)
            }
            ("fit_content", [Literal::Length(max)]) => size(*max).then_some(false),
            _ => None,
        },
        _ => None,
    }
}

fn items(value: &Literal) -> &[Literal] {
    match value {
        Literal::List(items) => items,
        value => std::slice::from_ref(value),
    }
}

/// Implicit tracks: one track or a list of them.
pub(super) fn auto_tracks(value: &Literal) -> bool {
    items(value).iter().all(|item| track(item).is_some())
}

/// An explicit track list of tracks, line names and `repeat(count, ...)`
/// with a whole count from 1, `auto_fill` or `auto_fit`. Repeats hold at least
/// one track and do not nest; one automatic repeat at most, and with it every
/// track must be fixed.
pub(super) fn template(value: &Literal) -> bool {
    let (mut automatic, mut fixed) = (0, true);
    let mut check = |item: &Literal| match track(item) {
        Some(f) => {
            fixed &= f;
            true
        }
        None => false,
    };
    for item in items(value) {
        match item {
            Literal::Call(function, arguments) if function == "repeat" => {
                let Some((count, inner)) = arguments.split_first() else {
                    return false;
                };
                match count {
                    Literal::Number(n) if n.fract() == 0.0 && (1.0..=65535.0).contains(n) => {}
                    Literal::Identifier(n) if n == "auto_fill" || n == "auto_fit" => automatic += 1,
                    _ => return false,
                }
                if !inner.iter().any(|item| !name(item))
                    || !inner.iter().all(|item| name(item) || check(item))
                {
                    return false;
                }
            }
            item if name(item) => {}
            item if check(item) => {}
            _ => return false,
        }
    }
    automatic == 0 || (automatic == 1 && fixed)
}

/// Named areas: a string per row with the same number of whitespace-separated
/// cells, `.` for an unnamed cell, and each name covering a rectangle.
pub(super) fn areas(value: &Literal) -> bool {
    let mut found: Vec<(&str, [usize; 4], usize)> = Vec::new();
    let mut columns = None;
    for (row, text) in items(value).iter().enumerate() {
        let Literal::String(text) = text else {
            return false;
        };
        let cells: Vec<_> = text.split_whitespace().collect();
        if cells.is_empty() || columns.is_some_and(|c| c != cells.len()) {
            return false;
        }
        columns = Some(cells.len());
        for (column, cell) in cells.into_iter().enumerate() {
            if cell.chars().all(|c| c == '.') {
                continue;
            }
            match found.iter_mut().find(|(n, ..)| *n == cell) {
                Some((_, [r0, c0, r1, c1], count)) => {
                    (*r0, *c0, *r1, *c1) =
                        ((*r0).min(row), (*c0).min(column), row, (*c1).max(column));
                    *count += 1;
                }
                None => found.push((cell, [row, column, row, column], 1)),
            }
        }
    }
    let rows = items(value).len();
    rows > 0
        && rows < usize::from(u16::MAX)
        && columns.is_some_and(|c| c < usize::from(u16::MAX))
        && found
            .iter()
            .all(|(_, [r0, c0, r1, c1], count)| (r1 - r0 + 1) * (c1 - c0 + 1) == *count)
}

/// A nonzero whole line or a name, or `[start, end]`: a line, `auto` or a
/// name, then a positive span or an end line name.
pub(super) fn placement(value: &Literal) -> bool {
    let whole = |value: &Literal, low: f32, high: f32| matches!(value, Literal::Number(n) if n.fract() == 0.0 && (low..=high).contains(n));
    let line = |value: &Literal| whole(value, -32768.0, 32767.0) && *value != Literal::Number(0.0);
    match value {
        Literal::List(items) => {
            items.len() == 2
                && (line(&items[0])
                    || name(&items[0])
                    || items[0] == Literal::Identifier("auto".into()))
                && (whole(&items[1], 1.0, 65535.0) || name(&items[1]))
        }
        value => line(value) || name(value),
    }
}
