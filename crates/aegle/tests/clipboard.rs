//! Event handlers read named controls and copy text to the clipboard, the
//! same way through `ui!` and `Program::load`.
#![cfg(feature = "markup")]

use aegle::loader::{Program, State};
use aegle::prelude::*;
use aegle::ui::{ClipboardRequest, Size, TextSystem, Ui};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(400.0, 600.0));
    Ok(ui)
}

/// Clicks `button` and returns the text it offered to the clipboard.
fn copied(ui: &Ui, button: &Button) -> Result<Option<String>> {
    button.activate();
    ui.dispatch_callbacks()?;
    Ok(match ui.take_clipboard() {
        Some(ClipboardRequest::Write(text)) => Some(text),
        _ => None,
    })
}

/// Buttons are found in document order: the ones of `for` rows and the
/// component's come between the named ones.
fn drive(
    ui: &Ui,
    order: Label,
    name: TextField,
    buttons: [Button; 6],
    copies: State<i64>,
) -> Result {
    ui.refresh()?;
    let [copy_order, copy_name, copy_self, question, _, done] = &buttons;
    assert_eq!(copied(ui, copy_order)?.as_deref(), Some("订单号 A-42"));
    assert_eq!(copies.get(), 1);
    // The current content is read when the button is clicked.
    order.set_text("订单号 B-7");
    assert_eq!(copied(ui, copy_order)?.as_deref(), Some("订单号 B-7"));
    name.set_text("Grace");
    assert_eq!(copied(ui, copy_name)?.as_deref(), Some("Name: Grace"));
    assert_eq!(copied(ui, copy_self)?.as_deref(), Some("Copy me"));
    // A `for` row reads a control declared after it.
    assert_eq!(copied(ui, question)?.as_deref(), Some("later?"));
    // A component's event handler runs in the document and reads its controls.
    assert_eq!(copied(ui, done)?.as_deref(), Some("3 / 订单号 B-7"));
    Ok(())
}

/// The buttons of `root` in document order.
fn buttons(root: &aegle::ui::Node) -> [Button; 6] {
    let state = root.state.upgrade().unwrap();
    let state = state.borrow();
    let mut found = Vec::new();
    let mut stack = vec![root.id];
    while let Some(id) = stack.pop() {
        let element = &state.tree.get(id).unwrap().context;
        if element.control.kind().name == "Button" {
            found.push(Button(aegle::ui::Node {
                state: root.state.clone(),
                id,
            }));
        }
        let children: Vec<_> = state.tree.children(id).unwrap().collect();
        stack.extend(children.into_iter().rev());
    }
    found.try_into().ok().expect("six buttons")
}

#[test]
fn handlers_copy_the_content_of_named_controls() -> Result {
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/copy.aegle")?;
    let all = buttons(&view.root);
    assert_eq!(all[0].text(), view.copy_order.text());
    drive(&compiled, view.order, view.name, all, view.copies)?;

    let loaded = ui()?;
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/copy.aegle");
    let view = Program::load(path)?.build(&loaded.root())?;
    let typed = |id: &str| view.handle(id).unwrap().clone();
    drive(
        &loaded,
        typed("order").typed().unwrap(),
        typed("name").typed().unwrap(),
        buttons(view.root().node()),
        view.state("copies").unwrap(),
    )
}
