//! Imports, components, states, bindings, events and blocks are resolved and typed.

use aegle_markup::{Bound, Child, ElementKind, ExprKind, Kind, Ref, compile};

fn files(entry: &str, library: &'static str) -> impl FnMut(&str) -> Result<String, String> {
    let entry = entry.to_owned();
    move |path| match path {
        "app/main.aegle" => Ok(entry.clone()),
        "lib/counter.aegle" => Ok(library.into()),
        _ => Err("not found".into()),
    }
}

const LIBRARY: &str = r#"
component Counter(start: int = 0, label: string) {
    state count: int = start
    Row {
        Text { text: label + ": " + str(count) }
        Button { text: "+1"; on clicked { count += 1 } }
    }
}"#;

#[test]
fn programs_resolve_imports_scopes_and_types() {
    let entry = r#"use "../lib/counter.aegle"
Window {
    state online: bool = true
    state items: list<string> = ["a", "b"]
    state ratio: float = 1
    Text { id: status; text: "ratio " + str(ratio * 2); visible: online }
    Counter { label: "clicks" }
    if online && len(items) > 0 { Text { text: "on" } }
    else if ratio < 0.5 { Text {} }
    else { Text { text: "off" } }
    for item in items { Text { text: item } }
    CheckBox { on changed { online = self.checked; if !online { items += ["c"] } } }
    Slider { value: ratio; on changed { ratio = self.value } }
}"#;
    let (program, files) = compile("app/main.aegle", &mut self::files(entry, LIBRARY)).unwrap();
    assert_eq!(files[1].path, "lib/counter.aegle");
    assert_eq!(program.templates.len(), 2);
    assert_eq!(program.ids, [("status".to_owned(), Kind::Text)]);
    let root = &program.templates[0].root;
    let Child::Element(status) = &root.children[0] else {
        panic!()
    };
    let Bound::Expr(visible) = &status.properties[1].1 else {
        panic!()
    };
    assert_eq!(visible.kind, ExprKind::Ref(Ref::State(0)));
    let Child::Element(counter) = &root.children[1] else {
        panic!()
    };
    assert_eq!(counter.kind, ElementKind::Component(1));
    assert!(counter.arguments[0].is_none() && counter.arguments[1].is_some());
    assert!(matches!(root.children[2], Child::If(..)));
    assert!(matches!(root.children[3], Child::For(..)));

    for (body, message) in [
        ("Text { text: missing }", "unknown name"),
        (
            "state n: int = 0; Text { text: n }",
            "expected string, found int",
        ),
        (
            "state n: int = 0; Text { text: str(n / 2.5) }",
            "expected float, found int",
        ),
        ("Button { on changed { } }", "no event"),
        (
            "state n: int = 0; Button { on clicked { m = 1 } }",
            "not a state",
        ),
        (
            "Text { text: self.text }",
            "only available in event handlers",
        ),
        (
            "state b: bool = true; if b { Text { id: x } }",
            "ids are only available",
        ),
        ("Text { radius: 1 + 1 }", "accepts only literal values"),
        ("Counter { start: 1 }", "missing argument"),
        ("Counter { label: 1 }", "expected string"),
        ("state n: int = 0; for i in n { }", "for requires a list"),
        (
            "state s: list<string> = []; state n: int = 0; Button { on clicked { s -= [\"a\"] } }",
            "does not apply",
        ),
    ] {
        let source = format!("use \"../lib/counter.aegle\"\nColumn {{ {body} }}");
        let error = compile("app/main.aegle", &mut self::files(&source, LIBRARY)).unwrap_err();
        assert!(error.0.contains(message), "{body}: {error}");
        assert!(error.0.starts_with("app/main.aegle:2:"), "{error}");
    }
    for (library, message) in [
        ("component Counter() { Counter {} }", "recursively"),
        ("use \"../app/main.aegle\"", "import cycle"),
        ("component Text() { Column {} }", "built-in name"),
        ("Column {}", "only declare components"),
        ("component Counter(a: int = a) { Column {} }", "literals"),
    ] {
        let source = "use \"../lib/counter.aegle\"\nColumn {}";
        let error = compile("app/main.aegle", &mut self::files(source, library)).unwrap_err();
        assert!(error.0.contains(message), "{library}: {error}");
    }
}
