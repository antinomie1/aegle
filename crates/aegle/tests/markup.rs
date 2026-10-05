//! Compiled markup shares the imperative tree, callbacks and weak lifetimes.
#![cfg(all(feature = "markup", feature = "motion"))]

use aegle::{Result, Size, TextSystem, Theme, Ui, UiError};
use aegle_text::{Blob, GenericFamily};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn compiled_view_uses_retained_state_and_local_layout_overrides() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let mut evaluations = 0;
    let view = aegle::ui!(
        {
            evaluations += 1;
            ui.root()
        },
        "tests/fixtures/panel.aegle"
    )?;
    assert_eq!(evaluations, 1);
    ui.resize(Size::new(320.0, 320.0))?;
    ui.refresh()?;
    assert!(!ui.has_animations());
    assert_eq!(view.panel.bounds()?, view.root.bounds()?);
    assert_eq!(view.panel.bounds()?.size.width, 240.0);
    assert_eq!(view.clear.bounds()?.size.width, 80.0);
    assert_eq!(
        view.clear.appearance()?.background,
        aegle::Color::rgb(103, 80, 164)
    );
    assert_eq!(
        view.editor.appearance()?.selection,
        aegle::Color::rgb(213, 223, 255)
    );
    assert_eq!(view.multiline.bounds()?.size.height, 70.0);
    let field = view.editor.clone();
    let status = view.status.clone();
    view.clear.on_click(move |_| {
        field.set_text("")?;
        status.set_text("Done")
    })?;
    view.clear.activate()?;
    ui.dispatch_callbacks()?;
    assert_eq!(view.editor.text()?, "");
    assert_eq!(view.status.text()?, "Done");
    let mut theme = Theme::dark();
    theme.control_height = 42.0;
    ui.set_theme(theme)?;
    ui.refresh()?;
    assert_eq!(view.clear.bounds()?.size, Size::new(80.0, 42.0));
    assert_eq!(view.multiline.bounds()?.size.height, 70.0);
    let retained = view.editor.clone();
    let container = view.root.clone();
    drop(view);
    assert!(retained.is_alive());
    container.remove()?;
    assert!(!retained.is_alive());
    let builder = aegle::ui!("tests/fixtures/panel.aegle");
    assert!(matches!(
        builder(&container).err().unwrap().downcast_ref::<UiError>(),
        Some(UiError::DeadHandle)
    ));
    assert!(ui.root().is_alive());
    Ok(())
}
