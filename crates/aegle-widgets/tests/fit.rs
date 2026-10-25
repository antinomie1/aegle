//! Lists, dialog cards, drawers and range controls stay inside a small window.
use aegle_text::GenericFamily;
use aegle_ui::{Container, Node, Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

const WINDOW: Size = Size::new(220.0, 200.0);

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(WINDOW);
    Ok(ui)
}

fn inside(node: &Node) -> bool {
    let b = node.bounds();
    b.origin.x >= 0.0
        && b.origin.y >= 0.0
        && b.origin.x + b.size.width <= WINDOW.width
        && b.origin.y + b.size.height <= WINDOW.height
}

fn parent(node: &Node) -> Node {
    let state = node.state.upgrade().unwrap();
    let id = state.borrow().tree.parent(node.id).unwrap().unwrap();
    Node {
        state: node.state.clone(),
        id,
    }
}

fn lines(container: &Container, count: usize) {
    for i in 0..count {
        container.text(&format!("Line {i} of a long list"));
    }
}

#[test]
fn overlays_and_rows_fit_a_small_window() -> Result {
    let ui = ui()?;
    let root = ui.root();
    // A popup list longer than the window keeps to it and scrolls.
    let anchor = root.button("Choose");
    let popup = anchor.popup();
    lines(&popup, 40);
    popup.show();
    ui.refresh()?;
    assert!(inside(&popup));
    assert!(popup.max_scroll_offset().y > 0.0);
    // Moving focus within it scrolls the focused control into view.
    let last = popup.button("Last");
    ui.refresh()?;
    last.focus();
    ui.refresh()?;
    assert!(popup.scroll_offset().y > 0.0);
    assert!(inside(&last));
    popup.hide();

    // A dialog card narrower and shorter than its 320 minimum and its body.
    let dialog = root.dialog("Settings");
    lines(&dialog.body(), 20);
    dialog.button("OK");
    dialog.show();
    ui.refresh()?;
    let card = parent(&dialog.body());
    assert!(inside(&card));
    assert_eq!(card.bounds().size.width, WINDOW.width - 32.0);
    assert!(card.max_scroll_offset().y > 0.0);
    dialog.close();

    // A drawer panel wider than the window takes the window's width.
    let drawer = root.drawer();
    lines(&drawer.content(), 20);
    drawer.open();
    ui.refresh()?;
    // The content container is the panel.
    let panel = drawer.content();
    assert!(inside(&panel));
    assert!(panel.max_scroll_offset().y > 0.0);
    drawer.close();

    // Range controls shrink in a narrow row instead of pushing past it, and a
    // crowded column keeps their thickness.
    let row = root.row();
    let slider = row.slider(0.0, 1.0, 0.5);
    let progress = row.progress(0.0, 1.0, 0.5);
    let field = row.text_field("name");
    // Squeezed narrower than its stepper strip, it still paints.
    let number = row.number_field(0.0, 10.0, 1.0);
    for _ in 0..8 {
        root.slider(0.0, 1.0, 0.5);
    }
    ui.refresh()?;
    for node in [&*slider, &*progress, &*field, &*number] {
        assert!(node.bounds().origin.x + node.bounds().size.width <= WINDOW.width);
    }
    assert_eq!(slider.bounds().size.height, 32.0);
    assert_eq!(progress.bounds().size.height, 16.0);
    Ok(())
}
