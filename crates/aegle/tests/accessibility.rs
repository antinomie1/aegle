//! Selecting only a system adapter feature still exports every stock widget's
//! semantics; without them UIA/AT-SPI saw unnamed containers instead of
//! labels and buttons.
#![cfg(any(
    feature = "accessibility",
    feature = "windows-accessibility",
    feature = "unix-accessibility"
))]

use aegle::{
    ui::{Result, Size, TextSystem, Theme, Ui, accesskit::Role},
    widgets::Widgets,
};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn stock_widgets_export_roles_and_names() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let root = ui.root();
    root.text("Title");
    root.button("Clear");
    root.check_box("Wrap", false);
    root.text_field("你好");
    ui.resize(Size::new(320.0, 200.0));
    let tree = ui.accessibility(true, "Window")?;
    let has = |role, name: &str| {
        tree.nodes.iter().any(|(_, node)| {
            node.role() == role && (node.label() == Some(name) || node.value() == Some(name))
        })
    };
    assert!(has(Role::Label, "Title"));
    assert!(has(Role::Button, "Clear"));
    assert!(has(Role::CheckBox, "Wrap"));
    assert!(
        tree.nodes
            .iter()
            .any(|(_, node)| node.role() == Role::TextInput)
    );
    Ok(())
}
