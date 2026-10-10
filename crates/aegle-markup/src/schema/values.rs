use super::{PropertyName, grid};
use crate::{ElementSpec, Layout, Styles, Value as Literal};

pub(crate) fn property_name(name: &str) -> Option<PropertyName> {
    use PropertyName::*;
    Some(match name {
        "title" => Title,
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
        "indicator_color" => IndicatorColor,
        "tooltip" => Tooltip,
        "paint_transition" => PaintTransition,
        "offset_transition" => OffsetTransition,
        "scale_transition" => ScaleTransition,
        "rotation_transition" => RotationTransition,
        "offset_x" => OffsetX,
        "offset_y" => OffsetY,
        "scale" => Scale,
        "rotation" => Rotation,
        "shadow" => Shadow,
        "background_gradient" => BackgroundGradient,
        "opacity" => Opacity,
        "backdrop_blur" => BackdropBlur,
        "shadow_transition" => ShadowTransition,
        "opacity_transition" => OpacityTransition,
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
        Theme => &["light", "dark", "high_contrast"],
        _ => &[],
    }
}

/// What a node property is checked against: the document's window or an element.
#[derive(Clone, Copy)]
pub(crate) enum Target<'s> {
    Window,
    Element(&'s ElementSpec<'s>),
}

impl<'s> Target<'s> {
    fn layout(self) -> Layout {
        match self {
            Target::Window => Layout::Flex,
            Target::Element(spec) => spec.layout,
        }
    }
    fn styles(self) -> Styles {
        match self {
            Target::Window => Styles::NONE,
            Target::Element(spec) => spec.styles,
        }
    }
    pub(crate) fn name(self) -> &'s str {
        match self {
            Target::Window => "Window",
            Target::Element(spec) => spec.name,
        }
    }
    fn container(self) -> bool {
        self.layout() != Layout::Leaf
    }
}

/// Whether a node property applies to a target, regardless of its value.
pub(crate) fn allowed(target: Target<'_>, name: PropertyName) -> bool {
    use PropertyName::*;
    let window = matches!(target, Target::Window);
    let styles = target.styles();
    match name {
        Title | Theme => window,
        FontSize => styles.contains(Styles::TEXT),
        SelectionColor | CaretColor => styles.contains(Styles::EDITOR),
        HoverBackground | FocusColor | FocusWidth => styles.contains(Styles::INTERACTIVE),
        PressedBackground => styles.contains(Styles::PRESSED),
        IndicatorColor => styles.contains(Styles::INDICATOR),
        Gap | Align | Justify | AlignContent => target.container(),
        Direction | Wrap => target.layout() == Layout::Flex,
        Columns | Rows | AutoColumns | AutoRows | Flow | JustifyItems | Areas => {
            target.layout() == Layout::Grid
        }
        Tooltip | MaxWidth | MaxHeight | AspectRatio | Margin | Inset | Shrink | Basis
        | AlignSelf | JustifySelf | GridColumn | GridRow | GridArea | OffsetX | OffsetY | Scale
        | Rotation | Shadow | Opacity | BackdropBlur => !window,
        _ => true,
    }
}

/// The easing curves a transition timing may name.
pub(crate) const EASINGS: &[&str] = &["linear", "ease_in", "ease_out", "ease_in_out"];

/// A duration, or a `[duration, easing]` pair.
fn timing(value: &Literal) -> bool {
    match value {
        Literal::Duration(_) => true,
        Literal::List(items) => {
            matches!(&items[..], [Literal::Duration(_), Literal::Identifier(easing)]
                if EASINGS.contains(&easing.as_str()))
        }
        _ => false,
    }
}

/// `token("package.name")`.
fn token(value: &Literal) -> bool {
    matches!(value, Literal::Call(function, arguments) if function == "token"
        && matches!(&arguments[..], [Literal::String(name)] if token_name(name)))
}

/// `[x, y, blur, spread, color]` with a nonnegative blur; a shadow token
/// replaces the whole list.
fn shadow(value: &Literal) -> bool {
    let Literal::List(items) = value else {
        return false;
    };
    match &items[..] {
        [x, y, Literal::Length(blur), spread, color] => {
            [x, y, spread]
                .iter()
                .all(|v| matches!(v, Literal::Length(_)))
                && *blur >= 0.0
                && matches!(color, Literal::Color(_))
        }
        _ => false,
    }
}

/// `linear(degrees, stops...)` or `radial(stops...)` with 2 to 16 stops,
/// each a color, a color token or `[color, percent]`; either every stop has a
/// nondecreasing position in `[0%, 100%]` or none does.
fn gradient(value: &Literal) -> bool {
    let stops = match value {
        Literal::Call(function, arguments) if function == "linear" => match &arguments[..] {
            [Literal::Number(_), stops @ ..] => stops,
            _ => return false,
        },
        Literal::Call(function, stops) if function == "radial" => &stops[..],
        _ => return false,
    };
    let color = |c: &Literal| matches!(c, Literal::Color(_)) || token(c);
    let positions: Vec<_> = stops
        .iter()
        .map(|stop| match stop {
            Literal::List(pair) => match &pair[..] {
                [c, Literal::Percent(p)] if color(c) && (0.0..=100.0).contains(p) => Some(Some(*p)),
                _ => None,
            },
            c => color(c).then_some(None),
        })
        .collect::<Option<_>>()
        .unwrap_or_default();
    (2..=16).contains(&stops.len())
        && positions.len() == stops.len()
        && (positions.iter().all(Option::is_none)
            || positions.iter().all(Option::is_some)
                && positions.windows(2).all(|pair| pair[0] <= pair[1]))
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

pub(crate) fn validate(
    target: Target<'_>,
    name: PropertyName,
    value: &Literal,
) -> Result<(), String> {
    use PropertyName::*;
    if !allowed(target, name) {
        return Err(format!("{name:?} is not supported on {}", target.name()));
    }
    let window = matches!(target, Target::Window);
    let valid = match (name, value) {
        (Title, Literal::String(text)) => text.len() <= 4000 && !text.contains('\0'),
        (Label | Tooltip, Literal::String(_)) => true,
        (Width | Height, Literal::Length(n)) if window => {
            n.is_finite() && *n > 0.0 && n.fract() == 0.0 && f64::from(*n) <= f64::from(u32::MAX)
        }
        (Width | Height, _) if window => false,
        (Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight | Basis, value) => {
            length(value, true, true)
        }
        (
            Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
            | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
            | IndicatorColor | BorderWidth | Radius | FocusWidth | FontSize | Padding | Gap,
            Literal::Call(function, _),
        )
        | (Shadow, Literal::Call(function, _))
            if function == "token" =>
        {
            token(value)
        }
        (Shadow, value) => shadow(value),
        (BackgroundGradient, value) => gradient(value),
        (Opacity, Literal::Number(n)) => (0.0..=1.0).contains(n),
        (BackdropBlur, Literal::Length(n)) => n.is_finite() && *n >= 0.0,
        (Padding, value) if target.container() => edges(value, true, false),
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
        | (Grow | Shrink, Literal::Number(n)) => n.is_finite() && *n >= 0.0,
        (AspectRatio, Literal::Number(n)) => n.is_finite() && *n > 0.0,
        (FontSize, Literal::Length(n)) => n.is_finite() && *n > 0.0,
        (OffsetX | OffsetY, Literal::Length(n)) | (Rotation, Literal::Number(n)) => n.is_finite(),
        (Scale, Literal::Number(n)) => *n > 0.0 && *n <= 1000.0,
        (
            Transition | PaintTransition | OffsetTransition | ScaleTransition | RotationTransition
            | ShadowTransition | OpacityTransition,
            value,
        ) => timing(value),
        (
            Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
            | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
            | IndicatorColor,
            Literal::Color(_),
        ) => true,
        (Visible | Enabled, Literal::Bool(_)) => true,
        (_, Literal::Identifier(value)) => choices(name).contains(&value.as_str()),
        _ => false,
    };
    if valid {
        return Ok(());
    }
    let expected = match name {
        Title => "a string of at most 4000 bytes without NUL".into(),
        Label | Tooltip => "a string".into(),
        Width | Height if window => "a positive whole dp length fitting u32".into(),
        Width | Height | MinWidth | MinHeight | MaxWidth | MaxHeight | Basis => {
            "a nonnegative dp length, a percentage, calc(...) or auto".into()
        }
        Padding if target.container() => "a nonnegative dp length or percentage, a list of two \
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
        Shadow => "[x, y, blur, spread, color] with dp lengths and a nonnegative blur, or \
            token(\"package.name\")"
            .into(),
        BackgroundGradient => "linear(degrees, stops...) or radial(stops...) with 2 to 16 \
            stops, each a color, token(\"package.name\") or [color, percent], with \
            nondecreasing positions on every stop or none"
            .into(),
        Opacity => "a number in [0, 1]".into(),
        BackdropBlur => "a nonnegative dp length".into(),
        Transition | PaintTransition | OffsetTransition | ScaleTransition | RotationTransition
        | ShadowTransition | OpacityTransition => format!(
            "milliseconds, or [milliseconds, easing] with easing one of {}",
            EASINGS.join(", ")
        ),
        OffsetX | OffsetY => "a finite dp length".into(),
        Scale => "a number in (0, 1000]".into(),
        Rotation => "finite degrees".into(),
        Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
        | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
        | IndicatorColor => "a #RRGGBB or #RRGGBBAA color or token(\"package.name\")".into(),
        Grow | Shrink => "a finite nonnegative number".into(),
        Visible | Enabled => "true or false".into(),
        Direction | LayoutDirection | Wrap | Align | Justify | AlignContent | AlignSelf
        | JustifySelf | JustifyItems | Flow | Theme => choices(name).join(", "),
    };
    Err(format!("{name:?} requires {expected}"))
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
