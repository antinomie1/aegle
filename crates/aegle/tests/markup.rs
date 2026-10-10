//! Compiled markup shares the imperative tree, callbacks and weak lifetimes.
#![cfg(all(feature = "markup", feature = "motion"))]

#[allow(unused_imports)]
use aegle::prelude::*;
use aegle::ui::{Point, Result, Size, TextSystem, Theme, Ui};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc};

fn ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())
}

#[test]
fn compiled_view_uses_retained_state_and_local_layout_overrides() -> Result {
    let ui = ui()?;
    let mut evaluations = 0;
    let view = aegle::ui!(
        {
            evaluations += 1;
            ui.root()
        },
        "tests/fixtures/panel.aegle"
    )?;
    assert_eq!(evaluations, 1);
    ui.resize(Size::new(320.0, 500.0));
    ui.refresh()?;
    assert!(!ui.has_animations());
    assert!(view.check.is_checked() && !view.switch.is_checked());
    assert!(view.radio.is_checked());
    assert_eq!(
        (view.slider.range(), view.slider.step(), view.slider.value()),
        ((10.0, 20.0), 3.0, 20.0)
    );
    view.slider.set_value(15.0);
    assert_eq!((view.slider.value(), view.progress.value()), (16.0, 10.0));
    assert_eq!(view.panel.bounds(), view.root.bounds());
    assert_eq!(view.panel.bounds().size.width, 240.0);
    assert_eq!(view.panel.offset(), Point::default());
    assert!(view.panel.content_size().height > view.panel.bounds().size.height);
    assert_eq!(view.clear.bounds().size.width, 80.0);
    assert_eq!(
        view.clear.appearance().background,
        aegle::ui::Color::rgb(103, 80, 164)
    );
    assert_eq!(
        view.editor.appearance().selection,
        aegle::ui::Color::rgb(213, 223, 255)
    );
    assert_eq!(view.multiline.bounds().size.height, 70.0);
    let field = view.editor.clone();
    let status = view.status.clone();
    view.clear.on_click(move |_| {
        field.set_text("");
        status.set_text("Done")
    });
    view.clear.activate();
    ui.dispatch_callbacks()?;
    assert_eq!(view.editor.text(), "");
    assert_eq!(view.status.text(), "Done");
    let mut theme = Theme::dark();
    theme.control_height = 42.0;
    ui.set_theme(theme);
    ui.refresh()?;
    assert_eq!(view.clear.bounds().size, Size::new(80.0, 42.0));
    assert_eq!(view.multiline.bounds().size.height, 70.0);
    let retained = view.editor.clone();
    let container = view.root.clone();
    drop(view);
    assert!(retained.is_alive());
    container.remove();
    assert!(!retained.is_alive());
    let builder = aegle::ui!("tests/fixtures/panel.aegle");
    let build = std::panic::AssertUnwindSafe(|| drop(builder(&container)));
    assert!(
        std::panic::catch_unwind(build).is_err(),
        "a removed parent panics"
    );
    assert!(ui.root().is_alive());
    Ok(())
}

#[test]
fn compiled_dynamic_view_types_states_imports_and_bindings() -> Result {
    let ui = ui()?;
    let view = aegle::ui!(&ui.root(), "tests/fixtures/counter.aegle")?;
    assert_eq!(
        (view.count.get(), view.tags.get()),
        (0, vec!["new".to_owned()])
    );
    view.add.activate();
    ui.dispatch_callbacks()?;
    assert_eq!(view.status.text(), "count 1");
    assert_eq!(view.tags.get(), ["new", "1"]);
    let (count, status) = (view.count.clone(), view.status.clone());
    drop(view); // Bindings live with the controls, not the view.
    count.set(41)?;
    assert_eq!(status.text(), "count 41");
    Ok(())
}

#[test]
fn compiled_views_hold_records_as_data_and_reach_shared_host_actions() -> Result {
    use aegle::loader::{Data, markup::Type};
    let audited = Rc::new(RefCell::new(Vec::new()));
    let sink = audited.clone();
    aegle::loader::action("audit", &[Type::Int], move |arguments| {
        sink.borrow_mut().extend_from_slice(arguments);
        Ok(())
    });
    let ui = ui()?;
    let view = aegle::ui!(&ui.root(), "tests/fixtures/records.aegle")?;
    view.add.activate();
    ui.dispatch_callbacks()?;
    assert_eq!(*audited.borrow(), [Data::Int(1)]);
    let Data::List(items) = view.items.get() else {
        panic!("a record list")
    };
    assert_eq!(items.len(), 2);
    assert!(view.items.set(Data::Bool(true)).is_err());
    Ok(())
}
