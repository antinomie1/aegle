use super::{CheckedNode, Kind, PropertyName};
use crate::{Error, Value as Literal};

pub(super) fn property_name(name: &str) -> Option<PropertyName> {
    use PropertyName::*;
    Some(match name {
        "title" => Title,
        "text" => Text,
        "width" => Width,
        "height" => Height,
        "min_width" => MinWidth,
        "min_height" => MinHeight,
        "padding" => Padding,
        "gap" => Gap,
        "grow" => Grow,
        "visible" => Visible,
        "enabled" => Enabled,
        "label" => Label,
        "read_only" => ReadOnly,
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
        "min" => Min,
        "max" => Max,
        "value" => Value,
        "step" => Step,
        "indicator_color" => IndicatorColor,
        _ => return None,
    })
}

pub(super) fn validate(kind: Kind, name: PropertyName, value: &Literal) -> Result<(), String> {
    use PropertyName::*;
    let allowed = match name {
        Title | Theme => kind == Kind::Window,
        Text | FontSize => matches!(
            kind,
            Kind::Text
                | Kind::Button
                | Kind::TextField
                | Kind::TextArea
                | Kind::CheckBox
                | Kind::Switch
        ),
        ReadOnly | SelectionColor | CaretColor => matches!(kind, Kind::TextField | Kind::TextArea),
        HoverBackground | FocusColor | FocusWidth => {
            matches!(
                kind,
                Kind::Button
                    | Kind::TextField
                    | Kind::TextArea
                    | Kind::CheckBox
                    | Kind::Switch
                    | Kind::Slider
            )
        }
        PressedBackground => matches!(
            kind,
            Kind::Button | Kind::CheckBox | Kind::Switch | Kind::Slider
        ),
        Checked => matches!(kind, Kind::CheckBox | Kind::Switch),
        Min | Max | Value => matches!(kind, Kind::Slider | Kind::Progress),
        Step => kind == Kind::Slider,
        IndicatorColor => matches!(
            kind,
            Kind::CheckBox | Kind::Switch | Kind::Slider | Kind::Progress
        ),
        Gap => matches!(kind, Kind::Window | Kind::Column | Kind::Row),
        _ => true,
    };
    if !allowed {
        return Err(format!("{name:?} is not supported on {kind:?}"));
    }
    let valid = match (name, value) {
        (Title, Literal::String(text)) => text.len() <= 4000 && !text.contains('\0'),
        (Text, Literal::String(text)) if kind == Kind::TextField => !text.contains([
            '\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}',
        ]),
        (Text | Label, Literal::String(_)) => true,
        (Width | Height, Literal::Length(n)) if kind == Kind::Window => {
            n.is_finite() && *n > 0.0 && n.fract() == 0.0 && f64::from(*n) <= f64::from(u32::MAX)
        }
        (Width | Height, Literal::Identifier(name)) => kind != Kind::Window && name == "auto",
        (
            Width | Height | MinWidth | MinHeight | Padding | Gap | BorderWidth | Radius
            | FocusWidth,
            Literal::Length(n),
        )
        | (Grow | Step, Literal::Number(n)) => n.is_finite() && *n >= 0.0,
        (Min | Max | Value, Literal::Number(n)) => n.is_finite(),
        (FontSize, Literal::Length(n)) => n.is_finite() && *n > 0.0,
        (Transition, Literal::Duration(_)) => true,
        (Easing, Literal::Identifier(name)) => {
            matches!(
                name.as_str(),
                "linear" | "ease_in" | "ease_out" | "ease_in_out"
            )
        }
        (
            Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
            | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
            | IndicatorColor,
            Literal::Color(_),
        ) => true,
        (Visible | Enabled | ReadOnly | Checked, Literal::Bool(_)) => true,
        (Theme, Literal::Identifier(name)) => {
            matches!(name.as_str(), "light" | "dark" | "high_contrast")
        }
        _ => false,
    };
    if valid {
        return Ok(());
    }
    let expected = match name {
        Title => "a string of at most 4000 bytes without NUL",
        Text if kind == Kind::TextField => "a string without hard line separators",
        Text | Label => "a string",
        Width | Height if kind == Kind::Window => "a positive whole dp length fitting u32",
        Width | Height => "a nonnegative dp length or auto",
        MinWidth | MinHeight | Padding | Gap | BorderWidth | Radius | FocusWidth => {
            "a nonnegative dp length"
        }
        FontSize => "a positive dp length",
        Transition => "nonnegative whole milliseconds with the ms suffix",
        Easing => "linear, ease_in, ease_out or ease_in_out",
        Background | Foreground | BorderColor | FocusColor | SelectionColor | CaretColor
        | HoverBackground | PressedBackground | DisabledBackground | DisabledForeground
        | IndicatorColor => "a #RRGGBB or #RRGGBBAA color",
        Grow | Step => "a finite nonnegative number",
        Min | Max | Value => "a finite number",
        Visible | Enabled | ReadOnly | Checked => "true or false",
        Theme => "light, dark or high_contrast",
    };
    Err(format!("{name:?} requires {expected}"))
}

pub(super) fn validate_range(node: &CheckedNode) -> Result<(), Error> {
    if !matches!(node.kind, Kind::Slider | Kind::Progress) {
        return Ok(());
    }
    let mut bounds = [0.0, 1.0];
    for property in &node.properties {
        let index = match property.name {
            PropertyName::Min => 0,
            PropertyName::Max => 1,
            _ => continue,
        };
        let Literal::Number(value) = property.value else {
            unreachable!()
        };
        bounds[index] = f64::from(value);
    }
    if bounds[0] >= bounds[1] {
        return Err(Error::new(node.span, "numeric range requires min < max"));
    }
    Ok(())
}

pub(super) fn valid_id(id: &str) -> bool {
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
