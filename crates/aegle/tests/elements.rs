//! A control library's element is checked and built like a built-in one:
//! literal and token properties in a static document, bindings, events,
//! typed ids, blocks and slots in a dynamic one, and the same element
//! registered for run-time loading.
#![cfg(feature = "markup")]

use std::{cell::Cell, cell::RefCell, ops::Deref, rc::Rc, sync::Arc};

use aegle_text::{Blob, GenericFamily};

use aegle::prelude::*;
use aegle::{
    loader::{Elements, Program},
    ui::{Size, TextSystem, Ui},
};

/// A selectable chip: a button that remembers whether it is selected.
#[derive(Clone)]
pub struct Chip {
    button: Button,
    selected: Rc<Cell<bool>>,
}

impl Deref for Chip {
    type Target = Button;
    fn deref(&self) -> &Button {
        &self.button
    }
}

impl Chip {
    fn new(parent: &Container, label: &str, selected: bool) -> Self {
        let chip = Self {
            button: parent.button(label),
            selected: Rc::new(Cell::new(false)),
        };
        chip.select(selected);
        chip
    }
    fn select(&self, selected: bool) {
        self.selected.set(selected);
        self.set_border_width(if selected { 2.0 } else { 1.0 });
    }
}

aegle::element! {
    /// A selectable chip.
    pub Chip {
        style text interactive pressed;
        create |parent, label: line = "", selected: bool = false| Chip::new(parent, label, selected);
        set label: line => |chip, text| chip.set_text(text);
        set selected: bool => |chip, on| chip.select(on);
        event toggled => |chip, run| chip.on_click(move |_| run());
        get selected: bool => |chip| chip.selected.get();
    }
}

fn fonts() -> Result<Rc<RefCell<TextSystem>>> {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    Ok(Rc::new(RefCell::new(fonts)))
}

fn ui(fonts: &Rc<RefCell<TextSystem>>) -> Result<Ui> {
    let ui = Ui::with_fonts(fonts.clone(), Theme::light())?;
    ui.resize(Size::new(400.0, 300.0));
    Ok(ui)
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn a_library_element_builds_from_both_paths() -> Result {
    aegle::ui::register_token("app.chip", |_| Color::rgba(10, 20, 30, 255))?;
    let fonts = fonts()?;
    let compiled = ui(&fonts)?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/chips.aegle")?;
    let chip: &Chip = &view.plain;
    assert!(chip.selected.get() && !view.other.selected.get());

    // The loader knows only the elements it is given.
    let missing = Program::load(fixture("chips.aegle")).unwrap_err();
    assert!(
        missing.to_string().contains("unknown component `Chip`"),
        "{missing}"
    );
    let elements = Elements::new().with::<Chip>();
    let loaded = ui(&fonts)?;
    let runtime = Program::load_with(fixture("chips.aegle"), &elements)?.build(&loaded.root())?;
    let plain: Chip = runtime.handle("plain").unwrap().typed().unwrap();
    assert!(plain.selected.get());
    compiled.refresh()?;
    loaded.refresh()?;
    let scenes = |ui: &Ui| -> Result<Vec<String>> {
        let mut scenes = Vec::new();
        ui.visit_scenes(|visit| {
            scenes.push(format!("{visit:?}"));
            Ok(())
        })?;
        Ok(scenes)
    };
    assert_eq!(scenes(&compiled)?, scenes(&loaded)?);
    Ok(())
}

#[test]
fn a_library_element_binds_state_and_raises_events() -> Result {
    let ui = ui(&fonts()?)?;
    let view = aegle::ui!(ui.root(), "tests/fixtures/chip_state.aegle")?;
    ui.refresh()?;
    let height = view.root.bounds().size.height;
    assert!(!view.toggle.selected.get());
    view.toggle.activate();
    ui.dispatch_callbacks()?;
    // The event ran with `self` reading the chip, and the binding followed.
    assert!(view.on.get());
    assert!(view.toggle.selected.get());
    // The `if` block inside the component's slot built another chip.
    ui.refresh()?;
    assert!(view.root.bounds().size.height > height);
    Ok(())
}
