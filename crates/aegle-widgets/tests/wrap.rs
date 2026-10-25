//! Tests for text wrapping on labels and toggle controls.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use aegle_text::GenericFamily;
use aegle_ui::{Result, Size, TextSystem, Theme, Ui};
use aegle_widgets::Widgets;

fn test_ui() -> Result<Ui> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())
}

#[test]
fn label_and_toggle_text_wrap() -> Result {
    let ui = test_ui()?;
    let root = ui.root();
    root.set_padding(0.0);

    let long_text =
        "This is a very long line of text intended to test wrapping in constrained containers.";
    let label = root.text(long_text);
    assert!(!label.is_wrap());

    // Without wrapping, resize container to narrow width and refresh
    ui.resize(Size::new(80.0, 300.0));
    ui.refresh()?;
    let unwrapped_height = label.bounds().size.height;

    // Enable wrapping
    label.set_wrap(true);
    assert!(label.is_wrap());
    ui.refresh()?;
    let wrapped_height = label.bounds().size.height;

    // Wrapped height should be taller than single line unwrapped height
    assert!(
        wrapped_height > unwrapped_height,
        "wrapped height ({wrapped_height}) should exceed unwrapped ({unwrapped_height})"
    );

    // Disable wrapping again
    label.set_wrap(false);
    assert!(!label.is_wrap());
    ui.refresh()?;
    assert_eq!(label.bounds().size.height, unwrapped_height);

    // Test checkbox wrap property
    let check = root.check_box(long_text, false);
    assert!(!check.is_wrap());
    check.set_wrap(true);
    assert!(check.is_wrap());
    ui.refresh()?;
    let check_wrapped_height = check.bounds().size.height;
    check.set_wrap(false);
    assert!(!check.is_wrap());
    ui.refresh()?;
    let check_unwrapped_height = check.bounds().size.height;
    assert!(check_wrapped_height > check_unwrapped_height);

    Ok(())
}
