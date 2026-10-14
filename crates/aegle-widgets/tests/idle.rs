//! Windows sharing one text system keep the application's idle text within
//! its budget: text nobody shows gives up its layout, least recently shown
//! first, and is shaped and drawn again when it is shown.
use std::{cell::RefCell, rc::Rc, sync::Arc};

use aegle_text::{Blob, GenericFamily};
use aegle_ui::{Node, Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::Widgets;

fn released(node: &Node) -> bool {
    node.change(|state, id| {
        let control = &state.tree.get(id).unwrap().context.control;
        Ok(control.paragraph().unwrap().is_released())
    })
}

/// Glyph runs this UI draws.
fn glyphs(ui: &Ui) -> Result<usize> {
    let mut runs = 0;
    ui.visit_scenes(|visit| {
        if let aegle_ui::Visit::Scene { scene, .. } = visit {
            runs += scene.glyph_runs().len();
        }
        Ok(())
    })?;
    Ok(runs)
}

#[test]
fn idle_text_is_released_across_windows_and_restored_when_shown() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let fonts = Rc::new(RefCell::new(fonts));
    let window = || -> Result<(Ui, Node, Vec<Node>)> {
        let ui = Ui::with_fonts(fonts.clone(), Theme::light())?;
        ui.resize(Size::new(200.0, 200.0));
        ui.root().text("你好");
        let hidden = ui.root().column();
        let labels = (0..3).map(|_| hidden.text("Aegle").0).collect();
        hidden.set_visible(false);
        ui.refresh()?;
        Ok((ui, hidden.0, labels))
    };
    let (first, hidden, labels) = window()?;
    let (second, _, others) = window()?;
    // Without a budget nothing is released.
    assert!(labels.iter().chain(&others).all(|l| !released(l)));
    // Two of the six idle labels fit. Each window, as it refreshes, releases
    // its own least recently shown text until the shared total fits.
    fonts.borrow_mut().set_idle_budget(10);
    first.refresh()?;
    assert_eq!(labels.iter().filter(|l| released(l)).count(), 1);
    assert_eq!(fonts.borrow().idle_bytes(), 10);
    second.refresh()?;
    assert!(others.iter().all(released));
    assert_eq!(fonts.borrow().idle_bytes(), 10);

    // Shown again, released text is shaped and drawn.
    let before = glyphs(&first)?;
    hidden.set_visible(true);
    first.refresh()?;
    assert!(labels.iter().all(|l| !released(l)));
    assert_eq!(glyphs(&first)?, before + 3);
    assert_eq!(fonts.borrow().idle_bytes(), 0);
    // A closed window withdraws its idle text.
    fonts.borrow_mut().set_idle_budget(None);
    first.root().column().text("Aegle").0.set_visible(false);
    first.refresh()?;
    assert_eq!(fonts.borrow().idle_bytes(), 0);
    fonts.borrow_mut().set_idle_budget(100);
    first.refresh()?;
    assert_eq!(fonts.borrow().idle_bytes(), 5);
    drop(first);
    drop(second);
    assert_eq!(fonts.borrow().idle_bytes(), 0);
    Ok(())
}
