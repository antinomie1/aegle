use super::{Kind, PropertyName, grid};
use crate::{Error, Span, Value as Literal};

pub(crate) fn property_name(name: &str) -> Option<PropertyName> {
    use PropertyName::*;
    Some(match name {
        "title" => Title,
        "text" => Text,
        "width" => Width,
        "height" => Height,
        "min_width" => MinWidth,
        "min_height" => MinHeight,
        "max_width" => MaxWidth,
        "max_height" => MaxHeight,
        "aspect_ratio" => AspectRatio,
        "padding" => Padding,
        "margin" => Margin,
        "inset" => Inset,
        "gap" => Gap,
        "grow" => Grow,
        "shrink" => Shrink,
        "basis" => Basis,
        "direction" => Direction,
        "layout_direction" => LayoutDirection,
        "wrap" => Wrap,
        "align" => Align,
        "justify" => Justify,
        "align_content" => AlignContent,
        "align_self" => AlignSelf,
        "justify_self" => JustifySelf,
        "justify_items" => JustifyItems,
        "columns" => Columns,
        "rows" => Rows,
        "auto_columns" => AutoColumns,
        "auto_rows" => AutoRows,
        "flow" => Flow,
        "grid_column" => GridColumn,
        "grid_row" => GridRow,
        "areas" => Areas,
        "grid_area" => GridArea,
        "visible" => Visible,
        "enabled" => Enabled,
        "label" => Label,
        "read_only" => ReadOnly,
        "password" => Password,
        "theme" => Theme,
        "background" => Background,
        "foreground" => Foreground,
        "border_color" => BorderColor,
        "border_width" => BorderWidth,
        "radius" => Radius,
        "focus_color" => FocusColor,
        "focus_width" => FocusWidth,
        "selection_color" => SelectionColor,
        "caret_color" => CaretColor,
        "hover_background" => HoverBackground,
        "pressed_background" => PressedBackground,
        "disabled_background" => DisabledBackground,
        "disabled_foreground" => DisabledForeground,
        "font_size" => FontSize,
        "transition" => Transition,
        "easing" => Easing,
        "checked" => Checked,
        "mixed" => Mixed,
        "min" => Min,
        "max" => Max,
        "value" => Value,
        "step" => Step,
        "indicator_color" => IndicatorColor,
        "orientation" => Orientation,
        "indeterminate" => Indeterminate,
        "tooltip" => Tooltip,
        "decimals" => Decimals,
        "ratio" => Ratio,
        "paint_transition" => PaintTransition,
        "offset_transition" => OffsetTransition,
        "scale_transition" => ScaleTransition,
        "rotation_transition" => RotationTransition,
        "offset_x" => OffsetX,
        "offset_y" => OffsetY,
        "scale" => Scale,
        "rotation" => Rotation,
        _ => return None,
    })
}

/// The identifiers an enum-valued property accepts, in Rust variant order;
/// empty for other properties. `ui!` names the variant `snake_case` →
/// `CamelCase`, and the runtime engine maps each one explicitly.
pub fn choices(name: PropertyName) -> &'static [&'static str] {
    use PropertyName::*;
    const ALIGN: &[&str] = &["start", "end", "center", "stretch", "baseline"];
    const JUSTIFY: &[&str] = &[
        "start",
        "end",
        "center",
        "stretch",
        "space_between",
        "space_around",
        "space_evenly",
    ];
    match name {
        Direction => &["row", "column", "row_reverse", "column_reverse"],
        LayoutDirection => &["ltr", "rtl"],
        Wrap => &["no_wrap", "wrap", "wrap_reverse"],
        Align | AlignSelf | JustifySelf | JustifyItems => ALIGN,
        Justify | AlignContent => JUSTIFY,
        Flow => &["row", "column", "row_dense", "column_dense"],
        Easing => &["linear", "ease_in", "ease_out", "ease_in_out"],
        Theme => &["light", "dark", "high_contrast"],
        Orientation => &["horizontal", "vertical"],
        _ => &[],
    }
}

/// Whether a property applies to a component kind, regardless of its value.
pub(crate) fn allowed(kind: Kind, name: PropertyName) -> bool {
    use PropertyName::*;
    let flex = matches!(
        kind,
        Kind::Window | Kind::Column | Kind::Row | Kind::ScrollView
    );
    match name {
        Title => matches!(kind, Kind::Window | Kind::Tab),
        Theme => kind == Kind::Window,
        Text | FontSize => matches!(
            kind,
            Kind::Text
                | Kind::Button
                | Kind::TextField
                | Kind::TextArea
                | Kind::CheckBox
                | Kind::Switch
                | Kind::RadioButton
        ),
        ReadOnly | SelectionColor | CaretColor => matches!(kind, Kind::TextField | Kind::TextArea),
        Password => kind == Kind::TextField,
        HoverBackground | FocusColor | FocusWidth => {
            matches!(
                kind,
                Kind::Button
                    | Kind::TextField
                    | Kind::TextArea
                    | Kind::CheckBox
                    | Kind::Switch
                    | Kind::RadioButton
                    | Kind::Slider
            )
        }
        PressedBackground => matches!(
            kind,
            Kind::Button | Kind::CheckBox | Kind::Switch | Kind::RadioButton | Kind::Slider
        ),
        Checked => matches!(kind, Kind::CheckBox | Kind::Switch | Kind::RadioButton),
        Mixed => kind == Kind::CheckBox,
        Min | Max | Value => matches!(kind, Kind::Slider | Kind::Progress | Kind::NumberField),
        Step => matches!(kind, Kind::Slider | Kind::NumberField),
        Orientation => matches!(kind, Kind::Slider | Kind::Progress | Kind::Splitter),
        Indeterminate => kind == Kind::Progress,
        Decimals => kind == Kind::NumberField,
        Ratio => kind == Kind::Splitter,
        Tooltip => kind != Kind::Window,
        IndicatorColor => matches!(
            kind,
            Kind::CheckBox | Kind::Switch | Kind::RadioButton | Kind::Slider | Kind::Progress
        ),
        Gap | Align | Justify | AlignContent => kind.is_container(),
        Direction | Wrap => flex,
        Columns | Rows | AutoColumns | AutoRows | Flow | JustifyItems | Areas => kind == Kind::Grid,
        MaxWidth | MaxHeight | AspectRatio | Margin | Inset | Shrink | Basis | AlignSelf
        | JustifySelf | GridColumn | GridRow | GridArea | OffsetX | OffsetY | Scale | Rotation => {
            kind != Kind::Window
        }
        _ => true,
    }
}

/// A duration, or a `[duration, easing]` pair.
fn timing(value: &Literal) -> bool {
    match value {
        Literal::Duration(_) => true,
        Literal::List(items) => {
            matches!(&items[..], [Literal::Duration(_), Literal::Identifier(easing)]
                if choices(PropertyName::Easing).contains(&easing.as_str()))
        }
        _ => false,
    }
}

/// A registered token name: dot-separated ASCII letters, digits, `_` and `-`
/// in two segments or more, as `aegle-ui` requires.
fn token_name(name: &str) -> bool {
    name.contains('.')
        && name.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

/// A finite length, percentage, `calc` sum or, where allowed, `auto`. A sum's
/// sign depends on the parent's size, so it is never rejected as negative.
fn length(value: &Literal, nonnegative: bool, auto: bool) -> bool {
    match value {
        Literal::Length(n) | Literal::Percent(n) => n.is_finite() && (!nonnegative || *n >= 0.0),
        Literal::Call(function, parts) if function == "calc" => {
            matches!(&parts[..], [Literal::Percent(p), Literal::Length(l)] if p.is_finite() && l.is_finite())
        }
        Literal::Identifier(name) => auto && name == "auto",
        _ => false,
    }
}

/// One length or a list of two or four in CSS order.
fn edges(value: &Literal, nonnegative: bool, auto: bool) -> bool {
    match value {
        Literal::List(items) => {
            matches!(items.len(), 2 | 4) && items.iter().all(|i| length(i, nonnegative, auto))
        }
        value => length(value, nonnegative, auto),
    }
}

pub(crate) fn validate(kind: Kind, name: PropertyName, value: &Literal) -> Result<(), String> {
    use PropertyName::*;
    if !allowed(kind, name) {
        return Err(format!("{name:?} is not supported on {kind:?}"));
    }
    let valid = match (name, value) {
        (Title, Literal::String(text)) => text.len() <= 4000 && !text.contains('\0'),
        (Text, Literal::String(text)) if kind == Kind::TextField => !text.contains([
            '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
        ]),
        (Text | Label | Tooltip, Literal::String(_)) => true,
        (Decimals, Literal::Number(n)) => n.fract() == 0.0 && (0.0..=9.0).contains(n),
        (Ratio, Literal::Number(n)) => (0.0..=1.0).contains(n),
        (Ratio, Literal::Percent(n)) => (0.0..=100.0).contains(n),
        (Width | Height, Literal::Length(n)) if kind == Kind::Window => {
            n.is_finite() && *n > 0.0 && n.fract() == 0.0 && f64::from(*n) <= f64::from(u32::MAX)
        }
        (Width | Height, _) if kind == Kind::Window => false,
        (Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight | Basis, value) => {
            length(value, true, true)
        }
        (
            Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
            | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
            | IndicatorColor | BorderWidth | Radius | FocusWidth | FontSize | Padding | Gap,
            Literal::Call(function, arguments),
        ) if function == "token" => {
            matches!(&arguments[..], [Literal::String(token)] if token_name(token))
        }
        (Padding, value) if kind.is_container() => edges(value, true, false),
        (Margin | Inset, value) => edges(value, false, true),
        (Gap, Literal::List(items)) => {
            items.len() == 2 && items.iter().all(|i| length(i, true, false))
        }
        (Gap, value) => length(value, true, false),
        (Columns | Rows, value) => grid::template(value),
        (AutoColumns | AutoRows, value) => grid::auto_tracks(value),
        (GridColumn | GridRow, value) => grid::placement(value),
        (Areas, value) => grid::areas(value),
        (GridArea, value) => grid::name(value),
        (Padding | BorderWidth | Radius | FocusWidth, Literal::Length(n))
        | (Grow | Shrink | Step, Literal::Number(n)) => n.is_finite() && *n >= 0.0,
        (AspectRatio, Literal::Number(n)) => n.is_finite() && *n > 0.0,
        (Min | Max | Value, Literal::Number(n)) => n.is_finite(),
        (FontSize, Literal::Length(n)) => n.is_finite() && *n > 0.0,
        (Transition, Literal::Duration(_)) => true,
        (OffsetX | OffsetY, Literal::Length(n)) | (Rotation, Literal::Number(n)) => n.is_finite(),
        (Scale, Literal::Number(n)) => *n > 0.0 && *n <= 1000.0,
        (PaintTransition | OffsetTransition | ScaleTransition | RotationTransition, value) => {
            timing(value)
        }
        (
            Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
            | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
            | IndicatorColor,
            Literal::Color(_),
        ) => true,
        (
            Visible | Enabled | ReadOnly | Password | Checked | Mixed | Indeterminate,
            Literal::Bool(_),
        ) => true,
        (_, Literal::Identifier(value)) => choices(name).contains(&value.as_str()),
        _ => false,
    };
    if valid {
        return Ok(());
    }
    let expected = match name {
        Title => "a string of at most 4000 bytes without NUL".into(),
        Text if kind == Kind::TextField => "a string without hard line separators".into(),
        Text | Label | Tooltip => "a string".into(),
        Decimals => "a whole number from 0 to 9".into(),
        Ratio => "a number from 0 to 1 or a percentage".into(),
        Width | Height if kind == Kind::Window => "a positive whole dp length fitting u32".into(),
        Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight | Basis => {
            "a nonnegative dp length, a percentage, calc(...) or auto".into()
        }
        Padding if kind.is_container() => "a nonnegative dp length or percentage, a list of two \
            or four, or token(\"package.name\")"
            .into(),
        Margin | Inset => "a dp length, percentage or auto, or a list of two or four".into(),
        Gap => "a nonnegative dp length or percentage, a [row, column] list or \
            token(\"package.name\")"
            .into(),
        Columns | Rows => "tracks (dp, %, fr, auto, min_content, max_content, minmax(dp, fr), \
            fit_content(dp)), line name strings and repeat(count, ...) with one automatic \
            repeat at most, whose list then has only fixed tracks"
            .into(),
        AutoColumns | AutoRows => "tracks, alone or in a list".into(),
        GridColumn | GridRow => {
            "a nonzero whole line or a name, or [line, auto or name, span or end name]".into()
        }
        Areas => "row strings of equally many cell names, each name a rectangle".into(),
        GridArea => "an area name string".into(),
        BorderWidth | Radius | FocusWidth => {
            "a nonnegative dp length or token(\"package.name\")".into()
        }
        Padding => "a nonnegative dp length or token(\"package.name\")".into(),
        AspectRatio => "a finite positive number".into(),
        FontSize => "a positive dp length or token(\"package.name\")".into(),
        Transition => "nonnegative whole milliseconds with the ms suffix".into(),
        PaintTransition | OffsetTransition | ScaleTransition | RotationTransition => format!(
            "milliseconds, or [milliseconds, easing] with easing one of {}",
            choices(Easing).join(", ")
        ),
        OffsetX | OffsetY => "a finite dp length".into(),
        Scale => "a number in (0, 1000]".into(),
        Rotation => "finite degrees".into(),
        Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
        | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
        | IndicatorColor => "a #RRGGBB or #RRGGBBAA color or token(\"package.name\")".into(),
        Grow | Shrink | Step => "a finite nonnegative number".into(),
        Min | Max | Value => "a finite number".into(),
        Visible | Enabled | ReadOnly | Password | Checked | Mixed | Indeterminate => {
            "true or false".into()
        }
        Direction | LayoutDirection | Wrap | Align | Justify | AlignContent | AlignSelf
        | JustifySelf | JustifyItems | Flow | Easing | Theme | Orientation => {
            choices(name).join(", ")
        }
    };
    Err(format!("{name:?} requires {expected}"))
}

/// Checks literal min/max of validated range properties.
pub(crate) fn validate_range<'a>(
    kind: Kind,
    properties: impl Iterator<Item = (PropertyName, &'a Literal)>,
    span: Span,
) -> Result<(), Error> {
    if !matches!(kind, Kind::Slider | Kind::Progress | Kind::NumberField) {
        return Ok(());
    }
    let mut bounds = [0.0, 1.0];
    for (name, value) in properties {
        let index = match name {
            PropertyName::Min => 0,
            PropertyName::Max => 1,
            _ => continue,
        };
        let Literal::Number(value) = value else {
            unreachable!()
        };
        bounds[index] = f64::from(*value);
    }
    if bounds[0] >= bounds[1] {
        return Err(Error::new(span, "numeric range requires min < max"));
    }
    Ok(())
}

pub(crate) fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(
            id,
            "_" | "root"
                | "as"
                | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "dyn"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
                | "abstract"
                | "become"
                | "box"
                | "do"
                | "final"
                | "gen"
                | "macro"
                | "override"
                | "priv"
                | "typeof"
                | "unsized"
                | "virtual"
                | "yield"
                | "try"
        )
}
