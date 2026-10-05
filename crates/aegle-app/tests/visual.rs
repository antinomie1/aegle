//! Virtual list realization, image controls and canvas painters.
use aegle_app::{
    Key, KeyInput, Modifiers, Point, Result, Size, TextSystem, Theme, Ui,
    scene::{Color, Command, Image, Rect, RoundedRect},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn press(ui: &Ui, key: Key) -> Result {
    ui.key(KeyInput {
        key,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    })
}

#[test]
fn virtual_rows_follow_the_viewport_in_order() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let built = Rc::new(RefCell::new(Vec::new()));
    let clicked = Rc::new(Cell::new(None));
    let (log, click) = (built.clone(), clicked.clone());
    let list = ui.root().list_view(20.0, 1000, move |row, index| {
        log.borrow_mut().push(index);
        let button = row.button("")?;
        let click = click.clone();
        button.on_click(move |_| {
            click.set(Some(index));
            Ok(())
        })?;
        Ok(())
    })?;
    assert!(ui.root().list_view(0.0, 1, |_, _| Ok(())).is_err());
    assert!(ui.root().list_view(1.0, 20_000_000, |_, _| Ok(())).is_err());
    list.set_height(Some(100.0))?;
    ui.resize(Size::new(200.0, 300.0))?;
    assert!(ui.refresh()?);
    assert_eq!(*built.borrow(), [0, 1, 2, 3, 4]);
    assert_eq!(list.max_offset()?.y, 19_900.0);

    ui.scroll_by(Point::new(10.0, 10.0), Point::new(0.0, 30.0))?;
    ui.refresh()?;
    assert_eq!(*built.borrow(), [0, 1, 2, 3, 4, 5, 6]);
    // Scrolling back rebuilds row 0 before the retained rows, preserving Tab order.
    ui.scroll_by(Point::new(10.0, 10.0), Point::new(0.0, -30.0))?;
    ui.refresh()?;
    assert_eq!(built.borrow().len(), 8);
    assert!(!ui.refresh()?);
    press(&ui, Key::Tab)?;
    press(&ui, Key::Tab)?;
    press(&ui, Key::Enter)?;
    ui.dispatch_callbacks()?;
    assert_eq!(clicked.get(), Some(1));

    list.set_count(3)?;
    list.reload()?;
    built.borrow_mut().clear();
    ui.refresh()?;
    assert_eq!(*built.borrow(), [0, 1, 2]);
    list.scroll_to(Point::new(0.0, 500.0))?;
    assert_eq!(list.offset()?.y, 0.0);
    list.remove()?;
    ui.refresh()?;
    Ok(())
}

#[test]
fn images_and_canvases_record_retained_scenes() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    let image = Image::new(3, 2, vec![255; 24])?;
    let view = ui.root().image(&image)?;
    let paints = Rc::new(Cell::new(0));
    let count = paints.clone();
    let canvas = ui.root().canvas(move |builder, size| {
        count.set(count.get() + 1);
        let shape = RoundedRect::new(Rect::new(0.0, 0.0, size.width, size.height), 0.0)?;
        builder.fill(shape, Color::BLACK)?;
        Ok(())
    })?;
    canvas.set_size(Some(40.0), Some(10.0))?;
    ui.resize(Size::new(200.0, 100.0))?;
    ui.refresh()?;
    assert_eq!(view.bounds()?.size, Size::new(3.0, 2.0));
    assert_eq!(view.image()?.id(), image.id());
    assert_eq!(paints.get(), 1);
    let mut images = 0;
    ui.visit_scenes(|scene, _, _| {
        images += scene
            .commands()
            .iter()
            .filter(|c| matches!(c, Command::Image { .. }))
            .count();
        Ok(())
    })?;
    assert_eq!(images, 1);
    assert!(!ui.refresh()?);
    canvas.invalidate()?;
    assert!(ui.refresh()?);
    assert_eq!(paints.get(), 2);
    view.set_image(&Image::new(5, 4, vec![0; 80])?)?;
    ui.refresh()?;
    assert_eq!(view.bounds()?.size, Size::new(5.0, 4.0));
    Ok(())
}
