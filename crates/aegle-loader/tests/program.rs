//! Imports, components, states, bindings, events and blocks are resolved and typed.

use aegle_loader::{
    Elements,
    markup::{
        Bound, Child, ElementKind, ElementSpec, ExprKind, File, Item, Limits, Program,
        ProgramError, Ref, check_program, parse, parse_with_limits,
    },
};

fn specs() -> Vec<ElementSpec<'static>> {
    Elements::new().specs()
}

fn compile(
    entry: &str,
    read: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<(Program, Vec<File>), ProgramError> {
    aegle_loader::markup::compile(entry, &specs(), read)
}

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
    assert_eq!(
        program.ids,
        [("status".to_owned(), ElementKind::Control(0))]
    );
    assert_eq!(program.elements[0], "Text");
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
        ("component Text() { Column {} }", "element name"),
        ("Column {}", "only declare components"),
        ("component Counter(a: int = a) { Column {} }", "literals"),
    ] {
        let source = "use \"../lib/counter.aegle\"\nColumn {}";
        let error = compile("app/main.aegle", &mut self::files(source, library)).unwrap_err();
        assert!(error.0.contains(message), "{library}: {error}");
    }
}

#[test]
fn records_events_slots_locals_and_host_calls_are_checked() {
    let check = |source: &'static str| {
        compile("main.aegle", &mut |_| Ok(source.to_owned())).map(|(program, _)| program)
    };
    let program = check(
        r#"record Task { id: int; title: string }
component Box() {
    event picked(int)
    Column { slot; Button { on clicked { let n = 1; emit picked(n) } } }
}
Column {
    state tasks: list<Task> = [Task(1, "a")]
    for task in tasks key task.id { Box { on picked(v) { host.log(v, task.title) } Text { text: task.title } } }
}"#,
    )
    .unwrap();
    assert_eq!(program.records[0].name, "Task");
    assert_eq!(
        program.templates[1].events,
        [("picked".to_owned(), Some(aegle_markup::Type::Int))]
    );
    assert!(program.templates[1].slot && program.files == ["main.aegle"]);
    let call = &program.host_calls[0];
    assert_eq!(
        (call.name.as_str(), &call.types[..]),
        (
            "log",
            &[aegle_markup::Type::Int, aegle_markup::Type::String][..]
        )
    );

    for (body, message) in [
        (
            "record R { a: list<int> }\nColumn {}",
            "record fields are bool",
        ),
        ("Column { state x: Nope = 1 }", "unknown type `Nope`"),
        ("Column { slot }", "`slot` belongs once in a component body"),
        (
            "component C() { Column {} }\nColumn { C { Text {} } }",
            "this component has no slot",
        ),
        (
            "component C() { Column { Button { on clicked { emit gone } } } }\nColumn { C {} }",
            "declares no event `gone`",
        ),
        (
            "component C() { event e\nColumn { slot } }\nColumn { C { on e(v) {} } }",
            "this event carries no value",
        ),
        (
            "record T { id: int }\nColumn { state ts: list<T> = [T(1)]; for t in ts { Text {} } }",
            "add `key`",
        ),
        (
            "Column { state xs: list<float> = [1.5]; for x in xs { Text {} } }",
            "add `key`",
        ),
        (
            "Column { state n: int = 0; Button { on clicked { if true { let a = 1; n = a }; n = a } } }",
            "unknown name `a`",
        ),
        (
            "record T { id: int }\nColumn { state t: T = T(1, 2) }",
            "takes 1 values",
        ),
        (
            "record T { id: int }\nColumn { state t: T = T(1); Text { text: t.nope } }",
            "no field `nope`",
        ),
        (
            "Column { state count: int = 0; Text { id: count } }",
            "names both a control and a state",
        ),
    ] {
        let error = check(Box::leak(body.to_owned().into_boxed_str())).unwrap_err();
        assert!(error.0.contains(message), "{body}: {}", error.0);
    }
}

#[test]
fn both_checkers_stop_hand_built_documents_at_the_parse_ceiling() {
    // A main thread's stack: unoptimized checking at the ceiling needs more
    // than a test thread's 2 MiB.
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(ceiling)
        .unwrap()
        .join()
        .unwrap();
}

fn ceiling() {
    let deepest = |depth| {
        let source = "Column {".repeat(depth) + &"}".repeat(depth);
        let limits = Limits {
            max_depth: Limits::MAX_DEPTH,
            ..Limits::default()
        };
        parse_with_limits(&source, &limits).unwrap()
    };
    assert!(check_program(vec![deepest(Limits::MAX_DEPTH)], &specs()).is_ok());
    let deeper = || {
        let mut document = parse("Column {}").unwrap();
        let inner = deepest(Limits::MAX_DEPTH).root.unwrap();
        document
            .root
            .as_mut()
            .unwrap()
            .children
            .push(Item::Node(inner));
        document
    };
    let message = "nesting exceeds 256 levels";
    assert_eq!(
        check_program(vec![deeper()], &specs())
            .unwrap_err()
            .1
            .message,
        message
    );
}

#[test]
fn deep_types_and_expression_chains_are_rejected_not_overflowed() {
    let chain = |n: usize| format!("Column {{ gap: {} }}", vec!["1"; n].join(" + "));
    assert!(parse(&chain(20)).is_ok());
    assert!(parse(&chain(20_000)).is_err());
    let fields = format!("Column {{ gap: a{} }}", ".b".repeat(20_000));
    assert!(parse(&fields).is_err());
    let lists = format!(
        "record R {{ a: {}int{} }}",
        "list<".repeat(100_000),
        ">".repeat(100_000)
    );
    assert!(parse(&lists).is_err());
}

#[test]
fn integer_literals_beyond_i64_are_errors_not_floats() {
    assert!(parse("Column { gap: 9223372036854775807 }").is_ok());
    assert!(parse("Column { gap: 9223372036854775808 }").is_err());
}
