//! The cursor follows the control under the pointer, or the one that captured it.
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{
    Cursor, Modifiers, Node, Point, PointerId, PointerKind, Result, Size, TextSystem, Theme, Ui,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn center(node: &Node) -> Result<Point> {
    let bounds = node.bounds()?;
    Ok(Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    ))
}

fn pointer(ui: &Ui, kind: PointerKind, position: Point) -> Result {
    ui.pointer(PointerId(1), kind, position, Modifiers::default())
}

fn cursor_at(ui: &Ui, position: Point) -> Result<Cursor> {
    pointer(ui, PointerKind::Move, position)?;
    ui.cursor()
}

#[test]
fn cursor_tracks_text_fields_overrides_capture_and_scrollbars() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let root = ui.root();
    root.set_padding(0.0)?;
    root.set_gap(4.0)?;
    let label = root.text("说明")?;
    let button = root.button("OK")?;
    let field = root.text_field("Hello")?;
    let disabled = root.text_field("Off")?;
    disabled.set_enabled(false)?;
    let link = root.row()?;
    let hand = link.text("Open")?;
    let inner = link.text_field("In link")?;
    let area = root.text_area(&"line\n".repeat(12))?;
    area.set_height(Some(48.0))?;
    area.set_min_height(0.0)?;
    ui.resize(Size::new(240.0, 400.0))?;
    ui.refresh()?;

    assert_eq!(ui.cursor()?, Cursor::Default, "no pointer yet");
    assert_eq!(cursor_at(&ui, center(&label)?)?, Cursor::Default);
    assert_eq!(cursor_at(&ui, center(&button)?)?, Cursor::Default);
    assert_eq!(cursor_at(&ui, center(&field)?)?, Cursor::Text);
    field.set_read_only(true)?;
    assert_eq!(
        ui.cursor()?,
        Cursor::Text,
        "read-only text stays selectable"
    );
    assert_eq!(cursor_at(&ui, center(&disabled)?)?, Cursor::Default);

    // An explicit shape applies to descendants; a field's I-beam still wins over
    // an ancestor's, and its own override wins over both.
    link.set_cursor(Some(Cursor::Pointer))?;
    assert_eq!(link.cursor()?, Some(Cursor::Pointer));
    assert_eq!(cursor_at(&ui, center(&hand)?)?, Cursor::Pointer);
    assert_eq!(cursor_at(&ui, center(&inner)?)?, Cursor::Text);
    inner.set_cursor(Some(Cursor::Crosshair))?;
    assert_eq!(ui.cursor()?, Cursor::Crosshair);
    inner.set_cursor(None)?;
    link.set_cursor(None)?;
    assert_eq!(ui.cursor()?, Cursor::Text);
    assert_eq!(cursor_at(&ui, center(&hand)?)?, Cursor::Default);

    // A press captures the pointer, so the shape stays with the field while the
    // drag leaves it, and follows the pointer again after release.
    field.set_read_only(false)?;
    let inside = center(&field)?;
    pointer(&ui, PointerKind::Down { clicks: 1 }, inside)?;
    assert_eq!(cursor_at(&ui, center(&label)?)?, Cursor::Text);
    pointer(&ui, PointerKind::Up, center(&label)?)?;
    assert_eq!(ui.cursor()?, Cursor::Default);

    // The overlay scrollbar strip is an arrow even over a text area.
    let bounds = area.bounds()?;
    let body = Point::new(bounds.origin.x + 20.0, bounds.origin.y + 20.0);
    assert_eq!(cursor_at(&ui, body)?, Cursor::Text);
    let strip = Point::new(
        bounds.origin.x + bounds.size.width - 6.0,
        bounds.origin.y + 20.0,
    );
    assert_eq!(cursor_at(&ui, strip)?, Cursor::Default);

    // Hiding or removing the control under a stationary pointer updates it.
    let under = center(&field)?;
    assert_eq!(cursor_at(&ui, under)?, Cursor::Text);
    field.set_visible(false)?;
    assert_eq!(ui.cursor()?, Cursor::Default);
    field.set_visible(true)?;
    ui.refresh()?;
    assert_eq!(cursor_at(&ui, center(&field)?)?, Cursor::Text);
    field.remove()?;
    assert_eq!(
        ui.cursor()?,
        Cursor::Default,
        "a removed control cannot linger"
    );
    ui.pointer_leave()?;
    assert_eq!(ui.cursor()?, Cursor::Default);
    Ok(())
}
