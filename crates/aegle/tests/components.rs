//! Tabs, splitters, separators, number fields and the range variants build
//! the same controls from compiled and runtime-loaded markup, and their
//! change events reach markup state.
#![cfg(feature = "markup")]

#[allow(unused_imports)]
use aegle::prelude::*;
use aegle::{
    Key, KeyInput, Modifiers, Result, Size, TextSystem, Theme, Ui,
    loader::{Data, Program},
};
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
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.root().set_padding(0.0)?;
    ui.resize(Size::new(400.0, 300.0))?;
    Ok(ui)
}

#[test]
fn compiled_and_loaded_components_match() -> Result {
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/components.aegle")?;
    compiled.refresh()?;
    assert_eq!(view.split.ratio(), 0.25);
    assert!(view.progress.is_indeterminate()?);
    assert_eq!(
        (view.number.value()?, view.number.text()?.as_str()),
        (2.0, "2.0")
    );
    let slider = view.slider.bounds()?;
    assert!(slider.size.height > slider.size.width);
    assert_eq!(view.other.visible_bounds()?, None);

    let loaded = ui()?;
    let program = Program::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/components.aegle"
    ))?;
    let runtime = program.build(&loaded.root())?;
    loaded.refresh()?;
    let nodes = [
        ("tabs", &**view.tabs),
        ("edit", &view.edit),
        ("split", &view.split),
        ("slider", &view.slider),
        ("progress", &view.progress),
        ("line", &view.line),
        ("number", &view.number),
    ];
    for (id, node) in nodes {
        let handle = runtime.handle(id).unwrap();
        assert_eq!(handle.node().bounds()?, node.bounds()?, "{id}");
    }
    let split: Splitter = runtime.handle("split").unwrap().typed().unwrap();
    assert_eq!(split.ratio(), 0.25);
    Ok(())
}

#[test]
fn change_events_update_markup_state() -> Result {
    let ui = ui()?;
    let source = r#"
        Column {
            state page: int = 0
            state amount: float = 0
            Tabs {
                id: tabs
                on changed { page = self.selected }
                Tab { title: "A"; NumberField { id: number; max: 9; on changed { amount = self.value } } }
                Tab { title: "B"; Progress { indeterminate: page == 1; tooltip: "page " + str(page) } }
            }
        }
    "#;
    let program =
        Program::from_sources("events.aegle", &aegle::loader::Elements::new(), &mut |_| {
            Ok(source.to_owned())
        })?;
    let view = program.build(&ui.root())?;
    ui.refresh()?;
    let tabs: Tabs = view.handle("tabs").unwrap().typed().unwrap();
    let number: NumberField = view.handle("number").unwrap().typed().unwrap();
    let enter = || -> Result {
        for pressed in [true, false] {
            ui.key(KeyInput {
                key: Key::Enter,
                text: "",
                modifiers: Modifiers::default(),
                pressed,
                repeat: false,
            })?;
        }
        ui.dispatch_callbacks()
    };
    number.focus()?;
    number.change(|state, id| state.set_text(id, "4"))?;
    enter()?;
    assert_eq!(view.get("amount"), Some(Data::Float(4.0)));
    tabs.tab(1)?.focus()?;
    enter()?;
    assert_eq!(view.get("page"), Some(Data::Int(1)));
    Ok(())
}
