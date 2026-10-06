//! Shared helpers: drive a headless UI through its accessibility tree.
#![allow(dead_code)]
use aegle_access::accesskit::{Action, ActionRequest, NodeId, TreeId};
use aegle_app::{Result, Size, TextSystem, Theme, Ui};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// A 300 × 400 UI with the CJK test font as its only font.
pub fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    let families = families.iter().map(|(id, _)| *id);
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families);
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(300.0, 400.0))?;
    Ok(ui)
}

/// Visible text values and button labels with their stable semantic IDs.
pub fn texts(ui: &Ui) -> Result<Vec<(String, NodeId)>> {
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

pub fn click(ui: &Ui, label: &str) -> Result {
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

pub fn names(ui: &Ui) -> Result<Vec<String>> {
    Ok(texts(ui)?.into_iter().map(|(text, _)| text).collect())
}
