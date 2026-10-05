//! Structural syntax, UTF-8 diagnostics and bounded parsing.

use aegle_markup::{Limits, Value, parse, parse_with_limits};

#[test]
fn utf8_literals_delimiters_and_resource_boundaries() {
    let source = r#"// A full interface, with CRLF accepted as well.
Window {
    title: "你好\n\u4e16\u754c\ud83d\ude80\"\\\/\b\f\r\t"
    theme: dark; enabled: true; ratio: -1.25e+2; gap: 8dp
    surface: #A1b2C3; overlay: #12345678
    duration: 18446744073709551615ms; instant: 0ms
    Column {
        Text { text: "日本語" } // A comment preserves the line separator.
        Button { text: "OK"; enabled: false }
    }
}"#;
    let document = parse(source).unwrap();
    let root = &document.root;
    assert_eq!(root.name, "Window");
    assert_eq!(
        root.properties[0].value,
        Value::String("你好\n世界🚀\"\\/\u{8}\u{c}\r\t".into())
    );
    assert_eq!(root.properties[1].value, Value::Identifier("dark".into()));
    assert_eq!(root.properties[2].value, Value::Bool(true));
    assert_eq!(root.properties[3].value, Value::Number(-125.0));
    assert_eq!(root.properties[4].value, Value::Length(8.0));
    assert_eq!(root.properties[5].value, Value::Color([161, 178, 195, 255]));
    assert_eq!(root.properties[6].value, Value::Color([18, 52, 86, 120]));
    assert_eq!(root.properties[7].value, Value::Duration(u64::MAX));
    assert_eq!(root.properties[8].value, Value::Duration(0));
    assert_eq!(root.children[0].children.len(), 2);
    assert_eq!(
        &source[root.properties[4].value_span.start..root.properties[4].value_span.end],
        "8dp"
    );
    assert!(parse("Window {\r\nText {}\r\nButton {}\r\n}").is_ok());
    let exact = Limits {
        max_source_bytes: source.len(),
        max_depth: 3,
        max_nodes: 4,
    };
    assert!(parse_with_limits(source, &exact).is_ok());
    for (max_source_bytes, max_depth, max_nodes) in [
        (source.len() - 1, 3, 4),
        (source.len(), 2, 4),
        (source.len(), 3, 3),
        (source.len(), 257, 4),
    ] {
        let limits = Limits {
            max_source_bytes,
            max_depth,
            max_nodes,
        };
        assert!(parse_with_limits(source, &limits).is_err());
    }
    for bad in [
        "",
        "Window {",
        "Window {} Text {}",
        "Window { Text {} Button {} }",
        "Window { a: 1 b: 2 }",
        "Window { a: 1 + 2 }",
        "Window { a: str(count) }",
        "Window { state count: int = 0 }",
        "Window { a: 1e99 }",
        "Window { a: NaN }",
        "Window { a: 1e }",
        "Window { a: 1. }",
        "Window { a: - }",
        "Window { a: 0.5ms }",
        "Window { a: 1e2ms }",
        "Window { a: -1ms }",
        "Window { a: 18446744073709551616ms }",
        "Window { a: #fff }",
        "Window { a: #1234567 }",
        "Window { a: #12345z }",
        "Window { a: #123456789 }",
        r#"Window { a: "\ud800" }"#,
        r#"Window { a: "\ud800\u0041" }"#,
        r#"Window { a: "\udc00" }"#,
        r#"Window { a: "\u12zz" }"#,
        r#"Window { a: "\q" }"#,
        "Window { a: \"raw\nnewline\" }",
    ] {
        assert!(parse(bad).is_err(), "accepted {bad}");
    }
    let bad = "Window {\n text: \"你好\" + 1\n}";
    let diagnostic = parse(bad).unwrap_err().render(bad, "你好.aegle");
    assert!(diagnostic.starts_with("你好.aegle:2:13:"), "{diagnostic}");
    assert!(diagnostic.contains("expressions are not supported"));
}
