//! Validate a complete interface before any runtime construction takes place.

use aegle_markup::{Kind, PropertyName, check, parse};

#[test]
fn built_in_schema_rejects_invalid_documents_as_a_whole() {
    let source = r#"Window {
        id: main; title: "编辑器"; width: 640dp; height: 480dp; theme: dark
        padding: 12dp; gap: 8dp; min_width: 0dp; min_height: 1dp
        visible: true; enabled: true; label: "文档"
        Column {
            Row { gap: 4dp; grow: 1
                Button { id: save; text: "保存"; width: auto; height: 24dp }
                Text { id: status; text: "Ready" }
            }
            TextField { id: title; text: "你好"; read_only: false }
            TextArea { id: body; text: "第一行\n第二行"; read_only: true }
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
