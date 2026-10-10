//! The built-in element specs validate a complete interface before any
//! runtime construction takes place.

use aegle_loader::{
    Elements,
    markup::{Document, ElementKind, Error, Program, Prop, PropertyName, check_program, parse},
};

fn check(document: Document) -> Result<Program, Error> {
    check_program(vec![document], &Elements::new().specs()).map_err(|(_, error)| error)
}

#[test]
fn built_in_schema_rejects_invalid_documents_as_a_whole() {
    let source = r#"Window {
        id: main; title: "编辑器"; width: 640dp; height: 480dp; theme: dark
        padding: 12dp; gap: 8dp; min_width: 0dp; min_height: 1dp
        visible: true; enabled: true; label: "文档"
        background: #ffffff; foreground: #000000; border_color: #12345678
        border_width: 0dp; radius: 6dp
        transition: [120ms, ease_out]
        Column {
            Row { gap: 4dp; grow: 1
                Button { id: save; text: "保存"; width: auto; height: 24dp; font_size: 14dp
                    hover_background: #abcdef; pressed_background: #123456
                    focus_color: #abcdef; focus_width: 2dp
                    disabled_background: #778899; disabled_foreground: #000000
                    transition: [0ms, linear]
                }
                Text { id: status; text: "Ready"; offset_x: -2dp; offset_y: 1.5dp
                    scale: 1.25; rotation: -45; paint_transition: [80ms, ease_in]
                    offset_transition: 200ms; scale_transition: [0ms, linear]
                    rotation_transition: [1ms, ease_in_out]
                }
            }
            TextField { id: title; text: "你好"; read_only: false }
            ScrollView { height: 100dp; gap: 4dp; padding: 3dp
                TextArea { id: body; text: "第一行\n第二行"; read_only: true
                    selection_color: #33558880; caret_color: #112233
                }
            }
            CheckBox { text: "同意"; checked: true; mixed: true; font_size: 16dp; indicator_color: #123456 }
            RadioButton { text: "一"; checked: true; pressed_background: #123456 }
            Switch { text: "启用"; pressed_background: #123456; focus_width: 2dp }
            Slider { value: 200; max: 20; min: -10; step: 0.25; hover_background: #112233 }
            Progress { max: 10; min: 2; indicator_color: #123456; indeterminate: true }
            Separator { tooltip: "divider" }
            NumberField { min: 0; max: 9; step: 0.5; decimals: 2 }
            Tabs { Tab { title: "一"; Splitter { orientation: vertical; ratio: 30%; Text {}; Column {} } } }
        }
    }"#;
    let program = check(parse(source).unwrap()).unwrap();
    assert_eq!(program.ids[0], ("main".to_owned(), ElementKind::Window));
    let root = &program.templates[0].root;
    assert_eq!(root.properties[0].0, Prop::Node(PropertyName::Title));
    let ElementKind::Control(title) = program.ids[3].1 else {
        panic!("an element")
    };
    assert_eq!(program.elements[title], "TextField");
    assert!(check(parse("Column { width: auto; height: 0dp }").unwrap()).is_ok());
    for body in [
        "width: 0dp",
        "height: 1.5dp",
        "width: 4294967296dp",
        "width: auto",
        "width: 640",
        "padding: -1dp",
        "grow: 2dp",
        "enabled: 1",
        "theme: blue",
        "easing: linear",
        "transition: 120dp",
        "transition: [1ms, cubic]",
        "transition: [1ms, 100]",
        "offset_x: 1dp",
        "Text { offset_x: 1 }",
        "Text { rotation: 1dp }",
        "Text { scale: 0 }",
        "Text { scale: 1001 }",
        "Text { scale_transition: 1dp }",
        "Text { offset_transition: [1ms] }",
        "Text { paint_transition: [1ms, cubic] }",
        "Text { rotation_transition: [1ms, linear, linear] }",
        "Slider { step: -1 }",
        "Progress { step: 1 }",
        "CheckBox { checked: 1 }",
        "Tabs { Column {} }",
        "Column { Tab { title: \"x\" } }",
        "Tabs { Tab {} }",
        "Splitter { Text {} }",
        "NumberField { decimals: 10 }",
        "Splitter { ratio: 2; Text {}; Text {} }",
        "Slider { orientation: diagonal }",
        "Slider { indeterminate: true }",
        "tooltip: \"window\"",
        "Switch { Text {} }",
        "Switch { mixed: true }",
        "Slider { font_size: 12dp }",
        "Progress { hover_background: #112233 }",
        "Text { indicator_color: #112233 }",
        "ScrollView { font_size: 14dp }",
        "ScrollView { focus_width: 2dp }",
        "ScrollView { scroll_y: 100dp }",
        "background: true",
        "border_width: -1dp",
        "font_size: 14dp",
        "Text { font_size: 0dp }",
        "Text { font_size: 14 }",
        "Button { selection_color: #112233 }",
        "Text { hover_background: #112233 }",
        "TextField { pressed_background: #112233 }",
        "Column { focus_color: #112233 }",
        "focus_width: 2dp",
        "title: true",
        r#"title: "\u0000""#,
        "text: \"unsupported on Window\"",
        "gap: 1dp; gap: 2dp",
        "id: main; id: other",
        "id: root",
        "id: gen",
        "mispelled: 1",
        "Unknown {}",
        "Window {}",
        "Text { Button {} }",
        "Text { text: false }",
        "Text { title: \"wrong kind\" }",
        "Button { read_only: true }",
        "Text { gap: 1dp }",
        "Button { id: repeated }; Text { id: repeated }",
        r#"TextField { text: "bad\nline" }"#,
        r#"TextField { text: "bad\u2028line" }"#,
        "Text { text: str(count) }",
        "on clicked {}",
    ] {
        let source = format!("Window {{ {body} }}");
        assert!(parse(&source).and_then(check).is_err(), "accepted {source}");
    }
    let title = format!("Window {{ title: \"{}\" }}", "界".repeat(1334));
    assert!(parse(&title).and_then(check).is_err());
    let source = "Window {\n Text { text: true }\n}";
    let error = check(parse(source).unwrap()).unwrap_err();
    assert_eq!(&source[error.span.start..error.span.end], "true");
    assert!(
        error
            .render(source, "bad.aegle")
            .starts_with("bad.aegle:2:15:")
    );
}

#[test]
fn layout_properties_accept_lists_units_and_enums_and_reject_misuse() {
    let source = r#"Column {
        padding: [4dp, 8%]; gap: [2dp, 0dp]; direction: column_reverse; wrap: wrap
        align: center; justify: space_evenly; align_content: stretch
        Row { width: 50%; min_width: auto; max_width: 300dp; basis: 0dp; shrink: 0
            margin: [auto, -4dp, 0dp, 10%]; align_self: baseline; aspect_ratio: 1.5 }
        Grid { columns: [120dp, 1fr, 25%, auto, min_content, max_content]; rows: 2fr
            auto_rows: [20dp]; flow: column_dense; justify_items: end
            Text { text: "a"; grid_column: [auto, 2]; grid_row: -1; justify_self: start }
        }
        Grid { columns: ["a", repeat(auto_fit, minmax(80dp, 1fr), "b")]; areas: ["x x", ". y"]
            rows: [fit_content(40dp), repeat(2, 1fr)]; width: calc((100% - 8dp) / 2)
            Text { text: "a"; grid_area: "x"; grid_column: ["a", "b"]; grid_row: [2, "y"] }
        }
        Stack { Column { inset: [0dp, auto]; padding: 2dp; gap: token("app.gap") } }
        Button { text: "t"; background: token("app.fill"); radius: token("theme.radius")
            font_size: token("app.type-2") }
    }"#;
    let program = check(parse(source).unwrap()).unwrap();
    assert!(program.elements.iter().any(|name| name == "Grid"));
    for body in [
        "Column { padding: auto }",
        "Column { padding: [1dp, 2dp, 3dp] }",
        "Text { text: \"a\"; padding: [1dp, 2dp] }",
        "Column { gap: [1dp] }",
        "Column { justify: middle }",
        "Column { columns: [1fr] }",
        "Grid { columns: [-1fr] }",
        "Grid { Text { text: \"a\"; grid_column: 0 } }",
        "Grid { Text { text: \"a\"; grid_row: [1, 0] } }",
        "Grid { Text { text: \"a\"; grid_row: 1.5 } }",
        "Grid { direction: row }",
        "Column { layout_direction: up }",
        "Grid { columns: [repeat(auto_fill, 10dp), repeat(auto_fit, 10dp)] }",
        "Grid { columns: [1fr, repeat(auto_fill, 10dp)] }",
        "Grid { columns: [repeat(0, 10dp)] }",
        "Grid { columns: [repeat(2, \"a\")] }",
        "Grid { columns: [repeat(2, repeat(2, 1fr))] }",
        "Grid { areas: [\"a b\", \"a\"] }",
        "Grid { areas: [\"a b\", \"b a\"] }",
        "Grid { Text { text: \"a\"; grid_area: \"\" } }",
        "Column { width: calc(10dp * 2dp) }",
        "Column { areas: [\"a\"] }",
        "Column { aspect_ratio: 0 }",
        "Column { width: -5% }",
        "Column { margin: [1dp, x + 1] }",
        "Window { margin: 1dp }",
        "Column { background: token(\"plain\") }",
        "Column { background: token(\"a.\", \"b.c\") }",
        "Column { padding: token(\"a\") }",
    ] {
        assert!(parse(body).and_then(check).is_err(), "accepted {body}");
    }
}
