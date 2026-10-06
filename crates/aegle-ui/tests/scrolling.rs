//! Nested scroll boundaries share paint clips, capture, focus and composition.
use aegle_text::{Blob, GenericFamily, Selection, TextError};
use aegle_ui::{
    Appearance, ImeEdit, Modifiers, Point, PointerId, PointerKind, Result, Size, TextSystem, Theme,
    Ui,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

static PAINTS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn nested_viewports_preserve_records_and_editing_while_clipping_input() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let outer = ui.root().scroll_view()?;
    outer.set_size(Some(200.0), Some(100.0))?;
    outer.set_padding(8.0)?;
    outer.set_gap(4.0)?;
    let inner = outer.scroll_view()?;
    inner.set_size(Some(180.0), Some(60.0))?;
    inner.set_padding(4.0)?;
    inner.set_gap(4.0)?;
    let first = inner.button("first")?;
    let second = inner.button("second")?;
    let third = inner.button("third")?;
    for button in [&first, &second, &third] {
        button.set_height(Some(40.0))?;
    }
    first.set_skin(|theme, state| {
        PAINTS.fetch_add(1, Ordering::Relaxed);
        Appearance::new(theme, state)
    })?;
    let field = outer.text_area("你好\nsecond\nthird\nfourth\nfifth")?;
    field.set_height(Some(80.0))?;
    field.set_min_height(0.0)?;
    ui.resize(Size::new(240.0, 300.0))?;
    ui.refresh()?;
    assert_eq!(inner.max_offset()?.y, 76.0);
    assert_eq!(outer.max_offset()?.y, 60.0);
    assert!(third.visible_bounds()?.is_none());
    assert!(inner.scroll_to(Point::new(f32::NAN, 0.0)).is_err());
    let paints = PAINTS.load(Ordering::Relaxed);
    ui.scroll(Point::new(20.0, 20.0), 100.0)?;
    ui.refresh()?;
    assert_eq!((inner.offset()?.y, outer.offset()?.y), (76.0, 24.0));
    assert_eq!(
        PAINTS.load(Ordering::Relaxed),
        paints,
        "scroll rebuilt a child's record"
    );
    ui.visit_scenes(|_, transform, clip| {
        if let Some(clip) = clip {
            assert!(clip.origin.y >= 0.0 && clip.origin.y + clip.size.height <= 100.0);
            assert!(transform.coefficients().iter().all(|n| n.is_finite()));
        }
        Ok(())
    })?;

    outer.scroll_to(Point::default())?;
    inner.scroll_to(Point::default())?;
    let clicks = Rc::new(Cell::new(0));
    let count = clicks.clone();
    first.on_click(move |_| {
        count.set(count.get() + 1);
        Ok(())
    })?;
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        Point::new(20.0, 20.0),
        Modifiers::default(),
    )?;
    inner.scroll_to(Point::new(0.0, 76.0))?;
    ui.pointer(
        PointerId(1),
        PointerKind::Up,
        Point::new(20.0, 20.0),
        Modifiers::default(),
    )?;
    ui.dispatch_callbacks()?;
    assert_eq!(clicks.get(), 0, "clipped captured button activated");
    ui.pointer_leave()?;
    first.focus()?; // Explicit refocus also reveals an already focused node.
    ui.refresh()?;
    assert!(first.visible_bounds()?.is_some());
    third.focus()?;
    ui.refresh()?;
    assert!(third.visible_bounds()?.is_some());

    field.focus()?;
    field.select(Selection::default())?;
    ui.refresh()?;
    ui.take_ime_state(4000)?;
    ui.ime(ImeEdit {
        preedit: "世界",
        ..Default::default()
    })?;
    ui.refresh()?;
    ui.take_ime_state(4000)?;
    outer.scroll_to(Point::default())?;
    ui.refresh()?;
    let ime = ui.take_ime_state(4000)?.unwrap();
    assert!(!ime.reset);
    let anchor = ime.request.unwrap().cursor_rect;
    assert!(anchor.origin.y >= 0.0 && anchor.origin.y + anchor.size.height <= 100.0);
    assert_eq!(
        field
            .select(Selection::default())
            .unwrap_err()
            .downcast_ref(),
        Some(&TextError::CompositionActive)
    );
    ui.ime(ImeEdit {
        commit: Some("世界"),
        ..Default::default()
    })?;
    ui.refresh()?;
    assert!(field.text()?.starts_with("世界"));
    assert!(field.visible_bounds()?.is_some());

    #[cfg(feature = "accessibility")]
    {
        use aegle_access::accesskit::{Action, ActionData, ActionRequest, Role, TreeId};
        let snapshot = ui.accessibility(true, "Scroll")?;
        let (id, node) = snapshot
            .nodes
            .iter()
            .find(|(_, n)| n.role() == Role::ScrollView)
            .unwrap();
        assert!(node.clips_children());
        assert_eq!(node.scroll_y_max(), Some(66.0));
        assert!(ui.access_action(ActionRequest {
            action: Action::SetScrollOffset,
            target_tree: TreeId::ROOT,
            target_node: *id,
            data: Some(ActionData::SetScrollOffset(
                aegle_access::accesskit::Point::new(0.0, 30.0)
            )),
        })?);
        ui.refresh()?;
        assert_eq!(outer.offset()?.y, 30.0);
    }
    let retained = inner.offset()?;
    outer.set_visible(false)?;
    ui.refresh()?;
    assert_eq!(inner.offset()?, retained);
    outer.set_visible(true)?;
    ui.refresh()?;
    assert_eq!(inner.offset()?, retained);
    third.remove()?;
    ui.refresh()?;
    assert!(inner.offset()?.y <= inner.max_offset()?.y);
    inner.set_width(Some(300.0))?;
    ui.refresh()?;
    assert!(outer.max_offset()?.x > 0.0);
    outer.scroll_to(Point::new(f32::MAX, f32::MAX))?;
    assert_eq!(outer.offset()?, outer.max_offset()?);
    ui.refresh()?;
    assert!(!ui.refresh()?, "idle scroll view kept repainting");
    Ok(())
}

#[test]
fn overlay_scrollbar_drags_above_children_without_activating_them() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let view = ui.root().scroll_view()?;
    view.set_size(Some(200.0), Some(100.0))?;
    let clicks = Rc::new(Cell::new(0));
    for _ in 0..3 {
        let count = clicks.clone();
        let button = view.button("")?;
        button.set_height(Some(60.0))?;
        button.on_click(move |_| {
            count.set(count.get() + 1);
            Ok(())
        })?;
    }
    ui.resize(Size::new(240.0, 240.0))?;
    ui.refresh()?;
    let mut last = 0;
    ui.visit_scenes(|scene, _, _| {
        last = scene.commands().len();
        Ok(())
    })?;
    assert_eq!(
        last, 3,
        "border, track and thumb are not the topmost record"
    );
    let pointer = |kind, y| {
        ui.pointer(
            PointerId(1),
            kind,
            Point::new(195.0, y),
            Modifiers::default(),
        )
    };
    pointer(PointerKind::Down { clicks: 1 }, 2.0)?;
    pointer(PointerKind::Move, 1000.0)?;
    ui.refresh()?;
    assert_eq!(view.offset()?, view.max_offset()?);
    pointer(PointerKind::Move, -1000.0)?;
    pointer(PointerKind::Up, -1000.0)?;
    ui.dispatch_callbacks()?;
    assert_eq!((view.offset()?.y, clicks.get()), (0.0, 0));
    view.set_height(Some(400.0))?;
    ui.refresh()?;
    pointer(PointerKind::Down { clicks: 1 }, 20.0)?;
    pointer(PointerKind::Up, 20.0)?;
    ui.dispatch_callbacks()?;
    assert_eq!(clicks.get(), 1, "bar without overflow kept input");
    Ok(())
}

#[test]
fn overflowing_viewports_reserve_the_bar_and_draw_their_border_last() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    let view = ui.root().scroll_view()?;
    view.set_size(Some(200.0), Some(100.0))?;
    view.set_padding(4.0)?;
    let buttons = [view.button("")?, view.button("")?, view.button("")?];
    for button in &buttons {
        button.set_height(Some(60.0))?;
    }
    ui.resize(Size::new(240.0, 240.0))?;
    ui.refresh()?;
    let right = |node: &aegle_ui::Node| -> Result<f32> {
        let bounds = node.bounds()?;
        Ok(bounds.origin.x + bounds.size.width)
    };
    // The bar needs 14 pixels (track, margin and clearance); padding 4 would leave
    // it covering the buttons' right edge.
    assert_eq!(right(&buttons[0])?, 186.0, "content stops short of the bar");

    // The border is the first record of the viewport's overlay, drawn after every
    // child, so content scrolled under the edge cannot cover it.
    let mut last = None;
    ui.visit_scenes(|scene, _, _| {
        last = scene.commands().first().copied();
        Ok(())
    })?;
    assert!(matches!(
        last,
        Some(aegle_ui::scene::Command::Stroke { .. })
    ));

    // Without overflow no bar exists, so the padding is all that is reserved.
    view.set_height(Some(400.0))?;
    ui.refresh()?;
    assert_eq!(right(&buttons[0])?, 196.0);
    // The overflow returning reserves it again.
    view.set_height(Some(100.0))?;
    ui.refresh()?;
    assert_eq!(right(&buttons[0])?, 186.0);

    // Content that scrolls sideways or up must not slide under a bar either: it
    // is clipped where each overflowing axis's bar begins.
    buttons[0].set_width(Some(400.0))?;
    ui.refresh()?;
    view.scroll_to(Point::new(100.0, 30.0))?;
    ui.refresh()?;
    let seen = buttons[0].visible_bounds()?.unwrap();
    assert!(seen.origin.x + seen.size.width <= 186.0, "right bar");
    assert!(seen.origin.y + seen.size.height <= 86.0, "bottom bar");
    Ok(())
}
