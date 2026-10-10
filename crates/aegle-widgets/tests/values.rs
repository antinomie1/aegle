//! Shared value behavior connects pointer/keyboard input, callbacks and semantics.
use aegle_text::{Blob, GenericFamily, Selection};
use aegle_ui::{
    ImeEdit, Key, KeyInput, Modifiers, Point, PointerId, PointerKind, Result, Size, TextSystem,
    Theme, Ui,
};
use aegle_widgets::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

#[test]
fn value_controls_share_lifecycle_and_system_actions() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let group = ui.root().column();
    let toggles = group.row();
    for parent in [ui.root(), group.clone(), toggles.clone()] {
        parent.set_gap(0.0, 0.0);
    }
    let check = toggles.check_box("世界", false);
    let switch = toggles.switch("Hello", false);
    let slider = group.slider(0.0, 10.0, 3.0);
    slider.set_step(3.0);
    slider.set_accessible_label("Volume");
    let progress = group.progress(0.0, 10.0, 3.0);
    progress.set_accessible_label("Progress");
    let field = ui.root().text_field("Hello");
    assert!(panics(|| {
        progress.focus();
    }));
    let hover = aegle_ui::Style {
        hover_background: Some(aegle_ui::Color::BLACK),
        ..Default::default()
    };
    assert!(panics(|| {
        progress.set_style(hover);
    }));
    let changes = Rc::new(Cell::new(0));
    let count = changes.clone();
    let target = switch.clone();
    check.on_change(move |check| {
        count.set(count.get() + 1);
        target.set_checked(check.is_checked())
    });
    let target = progress.clone();
    slider.on_change(move |slider| target.set_value(slider.value()));
    ui.resize(Size::new(360.0, 360.0));
    ui.refresh()?;
    let width = check.bounds().size.width;
    ui.set_theme(Theme {
        gap: 40.0,
        ..Theme::light()
    });
    ui.refresh()?;
    assert!((check.bounds().size.width - width - 32.0).abs() < 0.001);
    ui.set_theme(Theme::light());
    let key = |key, pressed| {
        ui.key(KeyInput {
            key,
            pressed,
            text: "",
            modifiers: Modifiers::default(),
            repeat: false,
        })
    };
    check.focus();
    key(Key::Character(' '), true)?;
    assert!(!check.is_checked());
    key(Key::Character(' '), false)?;
    ui.dispatch_callbacks()?;
    assert!(check.is_checked() && switch.is_checked());
    assert_eq!(changes.get(), 1);
    check.set_checked(false);
    ui.dispatch_callbacks()?;
    assert_eq!(changes.get(), 1); // Model updates do not loop through user callbacks.
    slider.focus();
    slider.set_height(Some(1.0));
    ui.refresh()?;
    let mut tiny_focus = false;
    ui.visit_scenes(|visit| {
        let aegle_ui::Visit::Scene { scene, .. } = visit else {
            return Ok(());
        };
        tiny_focus |= scene.commands().iter().any(|command| matches!(command,
            aegle_scene::Command::Stroke { shape, color, width }
                if *color == Theme::light().accent && *width == 0.5 && shape.rect().size.height == 0.5));
        Ok(())
    })?;
    assert!(tiny_focus);
    slider.set_height(Some(36.0));
    key(Key::Right, true)?;
    ui.dispatch_callbacks()?;
    assert_eq!(progress.value(), 6.0);
    key(Key::End, true)?;
    slider.decrement();
    assert_eq!(slider.value(), 9.0);
    ui.refresh()?;
    let bounds = slider.bounds();
    let point = Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    );
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        point,
        Modifiers::default(),
    )?;
    ui.pointer(
        PointerId(1),
        PointerKind::Move,
        Point::new(bounds.origin.x + bounds.size.width + 30.0, point.y),
        Modifiers::default(),
    )?;
    assert_eq!(slider.value(), 10.0);
    group.set_enabled(false);
    ui.pointer(PointerId(1), PointerKind::Up, point, Modifiers::default())?;
    slider.decrement();
    assert_eq!(slider.value(), 10.0);
    check.toggle();
    assert!(!check.is_checked());
    group.set_enabled(true);
    assert!(panics(|| {
        slider.set_range(4.0, 4.0);
    }));
    assert!(panics(|| {
        slider.set_step(f64::NAN);
    }));
    assert_eq!(slider.range(), (0.0, 10.0));
    #[cfg(feature = "accessibility")]
    {
        use aegle_access::accesskit::{Action, ActionData, ActionRequest, Role, Toggled, TreeId};
        let tree = ui.accessibility(true, "Values")?;
        let find = |role| {
            tree.nodes
                .iter()
                .find(|(_, node)| node.role() == role)
                .unwrap()
        };
        assert_eq!(find(Role::CheckBox).1.toggled(), Some(Toggled::False));
        assert_eq!(find(Role::Switch).1.toggled(), Some(Toggled::True));
        assert_eq!(find(Role::Slider).1.numeric_value(), Some(10.0));
        assert_eq!(find(Role::Slider).1.numeric_value_step(), Some(3.0));
        assert_eq!(
            find(Role::ProgressIndicator).1.min_numeric_value(),
            Some(0.0)
        );
        let request = |action, data| ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: find(Role::Slider).0,
            data,
        };
        assert!(ui.access_action(request(
            Action::SetValue,
            Some(ActionData::NumericValue(7.0))
        ))?);
        ui.dispatch_callbacks()?;
        assert_eq!(slider.value(), 6.0);
        assert_eq!(progress.value(), 6.0);
        assert!(!ui.access_action(request(
            Action::SetValue,
            Some(ActionData::NumericValue(f64::NAN))
        ))?);
        slider.set_enabled(false);
        assert!(!ui.access_action(request(Action::Increment, None))?);
        slider.set_enabled(true);
    }
    field.focus();
    ui.refresh()?;
    ui.take_ime_state(4000);
    ui.ime(ImeEdit {
        preedit: "世界",
        ..Default::default()
    })?;
    slider.set_value(0.0);
    switch.set_checked(false);
    ui.set_theme(Theme::dark());
    ui.refresh()?;
    assert_eq!(field.text(), "Hello");
    field.select(Selection::default()); // Cancels the composition.
    assert_eq!(field.text(), "Hello");
    check.toggle();
    group.remove();
    ui.dispatch_callbacks()?;
    assert_eq!(changes.get(), 1); // Destroyed queued controls are never invoked.
    assert!(!slider.is_alive());
    ui.refresh()?;
    Ok(())
}

/// Whether `change` panics, as handle methods do on rejected values.
fn panics(change: impl FnOnce()) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(change)).is_err()
}
