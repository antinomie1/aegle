//! Dynamic markup keeps bindings, blocks, keyed rows and components current.

use aegle_access::accesskit::{Action, ActionRequest, NodeId, TreeId};
use aegle_app::{Result, Size, TextSystem, Theme, Ui};
use aegle_loader::{Data, FromHandle, Program};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc};

const ITEM: &str = r#"component Item(name: string, total: int) {
    state clicks: int = 0
    Button { text: name + str(clicks) + "/" + str(total); on clicked { clicks += 1 } }
}"#;

const MAIN: &str = r#"use "item.aegle"
Column {
    state count: int = 0
    state names: list<string> = ["a", "b"]
    Text { id: status; text: "count " + str(count) }
    Button { id: add; text: "add"; on clicked { count += 1; if count == 2 { names += ["c"] } } }
    if count % 2 == 0 { Text { text: "even" } } else { Text { text: "odd" } }
    for name in names { Item { name: name; total: count } }
}"#;

fn program(main: &str) -> Result<Program> {
    Program::from_sources("main.aegle", &mut |path| match path {
        "main.aegle" => Ok(main.into()),
        "item.aegle" => Ok(ITEM.into()),
        _ => Err("missing".into()),
    })
}

/// Visible text values and button labels with their stable semantic IDs.
fn texts(ui: &Ui) -> Result<Vec<(String, NodeId)>> {
    let update = ui.accessibility(true, "")?;
    let visible = update.nodes.iter().filter(|(_, node)| !node.is_hidden());
    let text = |(id, node): &(NodeId, aegle_access::accesskit::Node)| {
        Some((node.value().or(node.label())?.to_owned(), *id))
    };
    Ok(visible
        .filter_map(text)
        .filter(|(text, _)| !text.is_empty())
        .collect())
}

fn click(ui: &Ui, label: &str) -> Result {
    let (_, target_node) = texts(ui)?
        .into_iter()
        .find(|(text, _)| text == label)
        .unwrap();
    let request = ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node,
        data: None,
    };
    ui.access_action(request)?;
    ui.dispatch_callbacks()
}

fn names(ui: &Ui) -> Result<Vec<String>> {
    Ok(texts(ui)?.into_iter().map(|(text, _)| text).collect())
}

#[test]
fn bindings_blocks_rows_and_components_follow_state() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    let families = families.iter().map(|(id, _)| *id);
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families);
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(300.0, 400.0))?;
    let view = program(MAIN)?.build(&ui.root())?;
    assert_eq!(names(&ui)?, ["count 0", "add", "even", "a0/0", "b0/0"]);
    click(&ui, "a0/0")?; // Component-local state.
    click(&ui, "add")?; // Event block, binding, condition and a reactive parameter.
    assert_eq!(names(&ui)?, ["count 1", "add", "odd", "a1/1", "b0/1"]);
    click(&ui, "add")?; // Appends a keyed row; existing rows keep their state.
    assert_eq!(
        names(&ui)?,
        ["count 2", "add", "even", "a1/2", "b0/2", "c0/2"]
    );
    let before = texts(&ui)?;
    let names_state = view.state::<Vec<String>>("names").unwrap();
    names_state.set(vec!["c".into(), "a".into()])?; // Reorders and removes by key.
    let after = texts(&ui)?;
    assert_eq!(names(&ui)?, ["count 2", "add", "even", "c0/2", "a1/2"]);
    let id =
        |list: &[(String, NodeId)], label: &str| list.iter().find(|(t, _)| t == label).unwrap().1;
    assert_eq!(id(&before, "a1/2"), id(&after, "a1/2"));
    assert!(names_state.set(vec!["a".into(), "a".into()]).is_err()); // Duplicate keys.
    assert_eq!(names(&ui)?[3..], ["c0/2", "a1/2"]); // The block keeps its rows.
    view.set(
        "names",
        Data::List(["c".into(), "a".into()].map(Data::String).into()),
    )?;
    assert!(view.set("count", Data::Float(1.0)).is_err());
    view.set("count", Data::Int(i64::MAX))?;
    let add = aegle_app::Button::from_handle(view.handle("add").unwrap()).unwrap();
    add.activate()?;
    assert!(ui.dispatch_callbacks().is_err()); // Overflow stops the handler.
    let status = aegle_app::Label::from_handle(view.id(0)).unwrap();
    assert_eq!(status.text()?, format!("count {}", i64::MAX));

    // A reload keeps compatible states and replaces the controls atomically.
    let mut view = view;
    view.set("count", Data::Int(3))?;
    let failing = program("Column { state n: int = 9223372036854775807 + 1; Text {} }")?;
    assert!(view.reload(&failing).is_err());
    assert_eq!(names(&ui)?[0], "count 3");
    let edited = MAIN.replace("\"count \"", "\"total \"");
    view.reload(&program(&edited)?)?;
    assert_eq!(names(&ui)?[..3], ["total 3", "add", "odd"]);
    assert!(program("Column { state n: int = 0; Text { text: n } }").is_err());
    let broken = MAIN.replace("names += [\"c\"]", "names += [\"c\", \"c\"]");
    view.reload(&program(&broken)?)?;
    view.set("count", Data::Int(1))?;
    assert!(click(&ui, "add").is_err()); // The duplicate row error keeps the old rows.
    assert!(names(&ui)?.contains(&"c0/2".to_owned()));
    Ok(())
}
