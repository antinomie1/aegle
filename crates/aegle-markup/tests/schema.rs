//! Validate a complete interface before any runtime construction takes place.

use aegle_markup::{Kind, PropertyName, check, parse};

#[test]
fn built_in_schema_rejects_invalid_documents_as_a_whole() {
    let source = r#"Window {
        id: main; title: "编辑器"; width: 640dp; height: 480dp; theme: dark
        padding: 12dp; gap: 8dp; min_width: 0dp; min_height: 1dp
        visible: true; enabled: true; label: "文档"
        background: #ffffff; foreground: #000000; border_color: #12345678
        border_width: 0dp; radius: 6dp
        transition: 120ms; easing: ease_out
        Column {
            Row { gap: 4dp; grow: 1
                Button { id: save; text: "保存"; width: auto; height: 24dp; font_size: 14dp
                    hover_background: #abcdef; pressed_background: #123456
                    focus_color: #abcdef; focus_width: 2dp
                    disabled_background: #778899; disabled_foreground: #000000
                    easing: linear; transition: 0ms
                }
                Text { id: status; text: "Ready" }
            }
            TextField { id: title; text: "你好"; read_only: false }
            TextArea { id: body; text: "第一行\n第二行"; read_only: true
                selection_color: #33558880; caret_color: #112233
            }
            CheckBox { text: "同意"; checked: true; font_size: 16dp; indicator_color: #123456 }
            Switch { text: "启用"; pressed_background: #123456; focus_width: 2dp }
            Slider { value: 200; max: 20; min: -10; step: 0.25; hover_background: #112233 }
            Progress { max: 10; min: 2; indicator_color: #123456 }
        }
    }"#;
    let document = check(parse(source).unwrap()).unwrap();
    assert_eq!(document.root.id.as_deref(), Some("main"));
    assert_eq!(document.root.properties[0].name, PropertyName::Title);
    assert_eq!(document.root.children[0].children[1].kind, Kind::TextField);
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
        "transition: 1ms; easing: cubic",
        "transition: 1ms; easing: 100",
        "Slider { min: 1; max: 1 }",
        "Progress { min: 1; max: 0 }",
        "Slider { step: -1 }",
        "Progress { step: 1 }",
        "CheckBox { checked: 1 }",
        "Switch { Text {} }",
        "Slider { font_size: 12dp }",
        "Progress { hover_background: #112233 }",
        "Text { indicator_color: #112233 }",
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
        "state count: int = 0",
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
