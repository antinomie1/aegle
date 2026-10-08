//! Range variants, numeric fields, separators, tabs, splitters and tooltips
//! share focus, keyboard, wheel, frame and semantic behavior.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use aegle_text::{Blob, GenericFamily};
use aegle_ui::{
    Key, KeyInput, Modifiers, Point, PointerId, PointerKind, Result, Size, TextSystem, Theme, Ui,
};
use aegle_widgets::*;

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.resize(Size::new(400.0, 400.0))?;
    ui.root().set_padding(0.0)?;
    Ok(ui)
}

fn key(ui: &Ui, key: Key) -> Result {
    for pressed in [true, false] {
        ui.key(KeyInput {
            key,
            text: "",
            modifiers: Modifiers::default(),
            pressed,
            repeat: false,
        })?;
    }
    ui.dispatch_callbacks()
}

fn click(ui: &Ui, at: Point) -> Result {
    let (id, m) = (PointerId(1), Modifiers::default());
    ui.pointer(id, PointerKind::Move, at, m)?;
    ui.pointer(id, PointerKind::Down { clicks: 1 }, at, m)?;
    ui.pointer(id, PointerKind::Up, at, m)?;
    ui.dispatch_callbacks()
}

fn center(node: &aegle_ui::Node) -> Result<Point> {
    let b = node.bounds()?;
    Ok(Point::new(
        b.origin.x + b.size.width / 2.0,
        b.origin.y + b.size.height / 2.0,
    ))
}

#[test]
fn range_variants_number_fields_and_separators() -> Result {
    let ui = ui()?;
    let row = ui.root().row()?;
    let slider = row.slider(0.0, 100.0, 50.0)?;
    slider.set_orientation(Orientation::Vertical)?;
    let line = row.separator()?;
    let progress = row.progress(0.0, 1.0, 0.0)?;
    let rule = ui.root().separator()?;
    let number = ui.root().number_field(0.0, 10.0, 2.0)?;
    number.set_step(0.5)?;
    number.set_decimals(1)?;
    ui.refresh()?;
    assert_eq!(
        (line.bounds()?.size.width, rule.bounds()?.size.height),
        (1.0, 1.0)
    );
    let bounds = slider.bounds()?;
    assert!(bounds.size.height > bounds.size.width);
    // Bottom of a vertical slider is its minimum.
    let near_bottom = Point::new(
        center(&slider)?.x,
        bounds.origin.y + bounds.size.height - 2.0,
    );
    click(&ui, near_bottom)?;
    assert!(slider.value()? < 10.0);
    key(&ui, Key::Up)?;
    let raised = slider.value()?;
    // Focused, the wheel steps the slider instead of scrolling.
    ui.wheel(
        center(&slider)?,
        Point::new(0.0, -32.0),
        Modifiers::default(),
        Instant::now(),
    )?;
    assert!(slider.value()? > raised);

    progress.set_indeterminate(true)?;
    ui.refresh()?;
    assert!(progress.is_indeterminate()? && ui.wants_frames()?);
    progress.set_indeterminate(false)?;
    ui.run_frame(Instant::now())?;
    ui.refresh()?;
    #[cfg(feature = "motion")]
    {
        // A programmatic change eases in over a few frames, then frames stop.
        progress.set_value(1.0)?;
        ui.refresh()?;
        assert!(ui.wants_frames()?);
        ui.run_frame(Instant::now() + Duration::from_secs(1))?;
        ui.refresh()?;
        assert!(!ui.wants_frames()?);
        // After an idle period it starts now, not at the long-past last frame.
        let idle = self::ui()?;
        let bar = idle.root().progress(0.0, 1.0, 0.0)?;
        idle.run_frame(Instant::now() - Duration::from_secs(1))?;
        idle.refresh()?;
        bar.set_value(1.0)?;
        idle.refresh()?;
        idle.run_frame(Instant::now() + Duration::from_millis(16))?;
        idle.refresh()?;
        assert!(idle.wants_frames()?);
    }

    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    number.on_change(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    number.focus()?;
    assert_eq!(number.text()?, "2.0");
    key(&ui, Key::Up)?;
    assert_eq!((number.value()?, number.text()?.as_str()), (2.5, "2.5"));
    number.change(|state, id| state.set_text(id, "42"))?;
    key(&ui, Key::Enter)?;
    assert_eq!((number.value()?, changes.get()), (10.0, 2));
    number.change(|state, id| state.set_text(id, "not a number"))?;
    key(&ui, Key::Enter)?;
    assert_eq!((number.value()?, number.text()?.as_str()), (10.0, "10.0"));
    // The lower stepper sits in the bottom right corner.
    let b = number.bounds()?;
    click(
        &ui,
        Point::new(
            b.origin.x + b.size.width - 4.0,
            b.origin.y + b.size.height - 4.0,
        ),
    )?;
    assert_eq!(number.value()?, 9.5);
    number.set_value(-3.0)?;
    assert_eq!((number.value()?, changes.get()), (0.0, 3));
    assert!(number.set_decimals(10).is_err());
    Ok(())
}

#[test]
fn tabs_splitters_and_tooltips() -> Result {
    let ui = ui()?;
    let tabs = ui.root().tabs()?;
    tabs.set_height(300.0)?;
    let pages: Vec<_> = ["One", "Two", "Three"]
        .iter()
        .map(|title| tabs.add(title))
        .collect::<Result<_>>()?;
    let changed = Rc::new(Cell::new(None));
    let seen = changed.clone();
    tabs.on_change(move |tabs| {
        seen.set(Some(tabs.selected()?));
        Ok(())
    })?;
    ui.refresh()?;
    let visible = |pages: &[aegle_ui::Container]| -> Result<Vec<bool>> {
        pages
            .iter()
            .map(|p| p.visible_bounds().map(|b| b.is_some()))
            .collect()
    };
    assert_eq!(
        (tabs.selected()?, visible(&pages)?),
        (0, vec![true, false, false])
    );
    click(&ui, center(&tabs.tab(1)?.0)?)?;
    ui.refresh()?;
    assert_eq!(
        (changed.get(), visible(&pages)?),
        (Some(1), vec![false, true, false])
    );
    key(&ui, Key::Right)?;
    assert_eq!((tabs.selected()?, changed.get()), (2, Some(2)));
    tabs.select(0)?;
    assert_eq!((tabs.selected()?, changed.get()), (0, Some(2)));
    assert!(tabs.select(3).is_err());

    let split = pages[0].splitter(Orientation::Horizontal)?;
    split.set_size(300.0, 100.0)?;
    let button = split.first().button("Hint")?;
    ui.refresh()?;
    let half = split.first().bounds()?.size.width;
    let grip = split.first().bounds()?;
    let at = Point::new(grip.origin.x + grip.size.width + 3.0, grip.origin.y + 10.0);
    let (id, m) = (PointerId(1), Modifiers::default());
    ui.pointer(id, PointerKind::Move, at, m)?;
    ui.pointer(id, PointerKind::Down { clicks: 1 }, at, m)?;
    ui.pointer(id, PointerKind::Move, Point::new(at.x - 60.0, at.y), m)?;
    ui.pointer(id, PointerKind::Up, Point::new(at.x - 60.0, at.y), m)?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    assert!((split.first().bounds()?.size.width - (half - 60.0)).abs() < 2.0);
    key(&ui, Key::End)?;
    assert_eq!(split.ratio(), 1.0);
    assert!(split.set_ratio(1.5).is_err());
    // A collapsed pane clips its overflowing content.
    split.set_ratio(0.0)?;
    ui.refresh()?;
    assert_eq!(button.visible_bounds()?, None);
    split.set_ratio(0.5)?;
    ui.refresh()?;

    button.set_tooltip(Some("Shows a hint"))?;
    ui.pointer(id, PointerKind::Move, center(&button)?, m)?;
    let due = ui.next_wake()?.expect("a pending tooltip");
    let children = || {
        ui.root()
            .change(|state, id| Ok(state.tree.children(id)?.count()))
    };
    let before = children()?;
    ui.wake(due + Duration::from_millis(1))?;
    ui.refresh()?;
    assert_eq!(children()?, before + 1);
    key(&ui, Key::Escape)?;
    assert_eq!(children()?, before);
    #[cfg(feature = "accessibility")]
    {
        let tree = ui.accessibility(true, "")?;
        assert!(
            tree.nodes
                .iter()
                .any(|(_, n)| n.description() == Some("Shows a hint"))
        );
    }
    Ok(())
}
