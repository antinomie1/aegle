//! Right-to-left layout mirrors rows, scrolling, bars and directional controls,
//! and switching back restores the left-to-right arrangement.
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{
    Key, KeyInput, LayoutDirection, Modifiers, Point, PointerId, PointerKind, Result, Size,
    TextSystem, Theme, Ui, bar::FOOTPRINT,
};
use aegle_widgets::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn press(ui: &Ui, key: Key) -> Result {
    for pressed in [true, false] {
        ui.key(KeyInput {
            key,
            text: "",
            modifiers: Modifiers::default(),
            pressed,
            repeat: false,
        })?;
    }
    Ok(())
}

#[test]
fn right_to_left_mirrors_layout_scrolling_and_directional_controls() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.root().set_padding(0.0);
    let row = ui.root().row();
    let (first, second) = (row.button("first"), row.button("second"));
    let slider = ui.root().slider(0.0, 100.0, 50.0);
    slider.set_width(Some(200.0));
    slider.set_height(None);
    let view = ui.root().scroll_view();
    view.set_width(Some(100.0));
    view.set_height(Some(60.0));
    view.set_padding(0.0);
    let content = view.row();
    let wide = content.button("wide");
    wide.set_width(300.0);
    wide.set_height(100.0);
    ui.resize(Size::new(400.0, 400.0));
    ui.root().set_layout_direction(Some(LayoutDirection::Rtl));
    ui.refresh()?;

    assert_eq!(slider.layout_direction(), LayoutDirection::Rtl);
    let (a, b) = (first.bounds(), second.bounds());
    assert!(a.origin.x > b.origin.x, "the first child is rightmost");
    assert_eq!(a.origin.x + a.size.width, 400.0);

    // Offset zero shows the right (start) edge; the vertical bar's gutter is
    // on the left, and content overflowing leftward scrolls into view.
    let (frame, inner) = (view.bounds(), content.bounds());
    assert_eq!(inner.origin.x - frame.origin.x, FOOTPRINT);
    assert_eq!(inner.origin.x + inner.size.width, frame.origin.x + 100.0);
    let wide = content.bounds();
    let middle = Point::new(frame.origin.x + 50.0, frame.origin.y + 30.0);
    ui.scroll_by(middle, Point::new(-50.0, 0.0))?;
    ui.refresh()?;
    assert_eq!(view.offset().x, 50.0, "revealing the left grows the offset");
    assert_eq!(content.bounds().origin.x, wide.origin.x + 50.0);

    // A slider's minimum is at its right end, and Right moves toward it.
    let track = slider.bounds();
    let y = track.origin.y + track.size.height / 2.0;
    let near_right = Point::new(track.origin.x + track.size.width - 12.0, y);
    let mods = Modifiers::default();
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        near_right,
        mods,
    )?;
    ui.pointer(PointerId(1), PointerKind::Up, near_right, mods)?;
    assert!(slider.value() < 5.0);
    slider.set_value(50.0);
    press(&ui, Key::Right)?;
    assert_eq!(slider.value(), 49.0);
    press(&ui, Key::Left)?;
    assert_eq!(slider.value(), 50.0);

    // Back to left to right: the gutter returns to the right and the
    // application's left padding is intact.
    view.scroll_to(Point::new(0.0, 0.0));
    ui.root().set_layout_direction(None);
    ui.refresh()?;
    assert!(first.bounds().origin.x < second.bounds().origin.x);
    let inner = content.bounds();
    assert_eq!(inner.origin.x, view.bounds().origin.x);
    assert_eq!(inner.size.width, 100.0 - FOOTPRINT);
    Ok(())
}
