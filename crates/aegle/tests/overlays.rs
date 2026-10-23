//! Trees, dialogs, drawers and wrapping text are markup elements: `ui!`
//! checks and compiles them, `Program::load` checks and builds the same
//! files, and both drive the same controls.
#![cfg(feature = "markup")]

use std::{cell::RefCell, rc::Rc, sync::Arc};

use aegle::loader::{Program, State};
use aegle::prelude::*;
use aegle::ui::{Key, KeyInput, Modifiers, Size, TextSystem, Ui};
use aegle_text::{Blob, GenericFamily};

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

fn ui_with(fonts: &Rc<RefCell<TextSystem>>) -> Result<Ui> {
    let ui = Ui::with_fonts(fonts.clone(), Theme::light())?;
    ui.resize(Size::new(400.0, 300.0));
    Ok(ui)
}

fn ui() -> Result<Ui> {
    ui_with(&fonts()?)
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn press(ui: &Ui, key: Key) -> Result {
    ui.key(KeyInput {
        key,
        text: "",
        modifiers: Modifiers::default(),
        pressed: true,
        repeat: false,
    })?;
    ui.dispatch_callbacks()
}

/// Recorded scenes, without the process-wide ids of their chevron paths.
fn scenes(ui: &Ui) -> Result<Vec<String>> {
    let mut scenes = Vec::new();
    ui.visit_scenes(|visit| {
        let scene = format!("{visit:?}");
        let mut parts = scene.split("Path#");
        let mut plain = parts.next().unwrap_or_default().to_owned();
        for part in parts {
            plain.push_str(part.trim_start_matches(|c: char| c.is_ascii_digit()));
        }
        scenes.push(plain);
        Ok(())
    })?;
    Ok(scenes)
}

#[test]
fn a_static_tree_and_wrapping_text_build_alike_from_both_paths() -> Result {
    let fonts = fonts()?;
    let compiled = ui_with(&fonts)?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/tree.aegle")?;
    compiled.refresh()?;
    let (tree, docs, samples): (&TreeView, &TreeNode, &TreeNode) =
        (&view.tree, &view.docs, &view.samples);
    assert!(tree.is_visible());
    assert!(docs.is_expanded() && docs.children_container().is_visible());
    assert!(!samples.is_expanded() && !samples.children_container().is_visible());
    assert!(view.guide.is_selected() && view.guide.text() == "指南");
    // Nested nodes lay out below their parent's header.
    assert!(view.guide.bounds().origin.y > docs.bounds().origin.y);
    let label: &Label = &view.note;
    assert!(label.is_wrap() && view.agree.is_wrap());
    let color: &Dropdown = &view.color;
    assert_eq!((color.selected(), color.items().len()), (1, 3));

    let loaded = ui_with(&fonts)?;
    let runtime = Program::load(fixture("tree.aegle"))?.build(&loaded.root())?;
    let guide: TreeNode = runtime.handle("guide").unwrap().typed().unwrap();
    assert!(guide.is_selected());
    loaded.refresh()?;
    assert_eq!(scenes(&compiled)?, scenes(&loaded)?);

    // Wrapping is what makes the narrow column's text taller.
    let wrapped = label.bounds().size.height;
    label.set_wrap(false);
    compiled.refresh()?;
    assert!(wrapped > label.bounds().size.height * 1.5);
    Ok(())
}

/// The handles and states one scenario drives, from either path.
struct Overlays {
    show: Button,
    leaf: TreeNode,
    dialog: Dialog,
    drawer: Drawer,
    home: Button,
    dialog_open: State<bool>,
    drawer_open: State<bool>,
    picked: State<String>,
    closes: State<i64>,
}

fn drive(ui: &Ui, view: Overlays) -> Result {
    ui.refresh()?;
    assert!(!view.dialog.is_open() && !view.drawer.is_open());
    assert_eq!(view.drawer.side(), DrawerSide::Right);

    // A click event sets the state the dialog's `open` is bound to.
    view.show.activate();
    ui.dispatch_callbacks()?;
    assert!(view.dialog.is_open() && view.dialog_open.get());

    // Escape dismisses it; `on closed` brings the state back in line once.
    ui.refresh()?;
    press(ui, Key::Escape)?;
    assert!(!view.dialog.is_open() && !view.dialog_open.get());
    assert_eq!(view.closes.get(), 1);
    // Re-asserting a closed dialog does not raise `closed` again.
    view.dialog_open.set(false)?;
    ui.dispatch_callbacks()?;
    assert_eq!(view.closes.get(), 1);
    view.dialog_open.set(true)?;
    assert!(view.dialog.is_open());
    view.dialog_open.set(false)?;
    ui.dispatch_callbacks()?;
    assert_eq!(view.closes.get(), 2);

    // Selecting a tree node runs its handler with `self` reading the node.
    ui.refresh()?;
    view.leaf.focus();
    press(ui, Key::Enter)?;
    assert!(view.leaf.is_selected());
    assert_eq!(view.picked.get(), "叶");
    assert!(view.drawer.is_open() && view.drawer_open.get());

    // A button inside the drawer closes it through the binding.
    view.home.activate();
    ui.dispatch_callbacks()?;
    assert!(!view.drawer.is_open() && !view.drawer_open.get());
    Ok(())
}

#[test]
fn overlays_and_tree_events_follow_state_from_both_paths() -> Result {
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/overlays.aegle")?;
    assert!(view.folder.is_expanded());
    drive(
        &compiled,
        Overlays {
            show: view.show.clone(),
            leaf: view.leaf.clone(),
            dialog: view.dialog.clone(),
            drawer: view.drawer.clone(),
            home: view.home.clone(),
            dialog_open: view.dialog_open.clone(),
            drawer_open: view.drawer_open.clone(),
            picked: view.picked.clone(),
            closes: view.closes.clone(),
        },
    )?;

    let loaded = ui()?;
    let runtime = Program::load(fixture("overlays.aegle"))?.build(&loaded.root())?;
    let typed = |id: &str| runtime.handle(id).unwrap();
    drive(
        &loaded,
        Overlays {
            show: typed("show").typed().unwrap(),
            leaf: typed("leaf").typed().unwrap(),
            dialog: typed("dialog").typed().unwrap(),
            drawer: typed("drawer").typed().unwrap(),
            home: typed("home").typed().unwrap(),
            dialog_open: runtime.state("dialog_open").unwrap(),
            drawer_open: runtime.state("drawer_open").unwrap(),
            picked: runtime.state("picked").unwrap(),
            closes: runtime.state("closes").unwrap(),
        },
    )
}
