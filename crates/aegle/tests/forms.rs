//! Form gaps closed in markup: a Dropdown element, a TextField `changed`
//! event, TreeNodes from `if`/`for` blocks and ids on slot content, driven
//! the same way through `ui!` and `Program::load`.
#![cfg(feature = "markup")]

use std::{cell::RefCell, rc::Rc, sync::Arc};

use aegle::loader::{Program, State};
use aegle::prelude::*;
use aegle::ui::{ImeEdit, Key, KeyInput, Modifiers, Size, TextSystem, Ui};
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

fn ui(fonts: &Rc<RefCell<TextSystem>>) -> Result<Ui> {
    let ui = Ui::with_fonts(fonts.clone(), Theme::light())?;
    ui.resize(Size::new(400.0, 400.0));
    Ok(ui)
}

fn key(ui: &Ui, key: Key, text: &str) -> Result {
    let input = |pressed| KeyInput {
        key,
        text,
        modifiers: Modifiers::default(),
        pressed,
        repeat: false,
    };
    ui.key(input(true))?;
    ui.key(input(false))?;
    ui.dispatch_callbacks()
}

/// The handles and states one scenario drives, from either path.
struct Forms {
    sizes: Dropdown,
    name: TextField,
    top: TreeNode,
    size: State<i64>,
    picked: State<String>,
    typed: State<String>,
    edits: State<i64>,
    folders: State<Vec<String>>,
    choices: State<Vec<String>>,
}

fn drive(ui: &Ui, view: Forms) -> Result {
    ui.refresh()?;
    // The bound selection applied after the bound items, though written first.
    assert_eq!((view.sizes.selected(), view.sizes.text()), (1, "中".into()));
    view.sizes.focus();
    key(ui, Key::Enter, "")?;
    ui.refresh()?;
    key(ui, Key::Down, "")?;
    key(ui, Key::Enter, "")?;
    assert_eq!((view.size.get(), view.picked.get()), (2, "大".into()));
    view.size.set(0)?;
    assert_eq!(view.sizes.text(), "小");

    // New choices keep a still-valid index, else select the first; an empty
    // list is a located error that leaves the dropdown as it was.
    view.choices.set(vec!["S".into(), "M".into()])?;
    view.size.set(1)?;
    assert_eq!(
        (view.sizes.items(), view.sizes.text()),
        (vec!["S".into(), "M".into()], "M".into())
    );
    view.choices.set(vec!["XS".into()])?;
    assert_eq!((view.sizes.selected(), view.sizes.text()), (0, "XS".into()));
    let error = view.choices.set(Vec::new()).unwrap_err().to_string();
    assert!(
        error.contains("`items` requires a nonempty list") && error.contains("forms.aegle"),
        "{error}"
    );
    assert_eq!(view.sizes.items(), ["XS"]);

    // Typing, an IME commit and deletion each raise `changed`; preedit and
    // programmatic text do not.
    view.name.focus();
    key(ui, Key::Character('a'), "a")?;
    ui.ime(ImeEdit {
        preedit: "名",
        ..Default::default()
    })?;
    ui.dispatch_callbacks()?;
    assert_eq!((view.typed.get(), view.edits.get()), ("a".into(), 1));
    ui.ime(ImeEdit {
        commit: Some("名"),
        ..Default::default()
    })?;
    ui.dispatch_callbacks()?;
    assert_eq!((view.typed.get(), view.edits.get()), ("a名".into(), 2));
    view.name.set_text("reset");
    ui.dispatch_callbacks()?;
    assert_eq!(view.edits.get(), 2);
    key(ui, Key::Backspace, "")?;
    assert_eq!((view.typed.get(), view.edits.get()), ("rese".into(), 3));

    // Rows of the `for` nest under the node around the block.
    ui.refresh()?;
    let rows = view.top.children_container().bounds().size.height;
    view.folders
        .set(vec!["文档".into(), "示例".into(), "新".into()])?;
    ui.refresh()?;
    assert!(view.top.children_container().bounds().size.height > rows);
    view.top.set_expanded(false);
    ui.refresh()?;
    assert!(!view.top.children_container().is_visible());
    Ok(())
}

#[test]
fn dropdowns_text_changes_tree_blocks_and_slot_ids_from_both_paths() -> Result {
    let fonts = fonts()?;
    let compiled = ui(&fonts)?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/forms.aegle")?;
    drive(
        &compiled,
        Forms {
            sizes: view.sizes.clone(),
            name: view.name.clone(),
            top: view.top.clone(),
            size: view.size.clone(),
            picked: view.picked.clone(),
            typed: view.typed.clone(),
            edits: view.edits.clone(),
            folders: view.folders.clone(),
            choices: view.choices.clone(),
        },
    )?;

    let loaded = ui(&fonts)?;
    let path = format!("{}/tests/fixtures/forms.aegle", env!("CARGO_MANIFEST_DIR"));
    let runtime = Program::load(path)?.build(&loaded.root())?;
    let handle = |id: &str| runtime.handle(id).unwrap();
    drive(
        &loaded,
        Forms {
            sizes: handle("sizes").typed().unwrap(),
            name: handle("name").typed().unwrap(),
            top: handle("top").typed().unwrap(),
            size: runtime.state("size").unwrap(),
            picked: runtime.state("picked").unwrap(),
            typed: runtime.state("typed").unwrap(),
            edits: runtime.state("edits").unwrap(),
            folders: runtime.state("folders").unwrap(),
            choices: runtime.state("choices").unwrap(),
        },
    )
}

fn build(ui: &Ui, source: &'static str) -> Result {
    let program = Program::from_sources("tree.aegle", &Default::default(), &mut |_| {
        Ok(source.to_owned())
    })?;
    program.build(&ui.root())?;
    ui.refresh()?;
    Ok(())
}

fn scenes(ui: &Ui) -> Result<Vec<String>> {
    let mut scenes = Vec::new();
    ui.visit_scenes(|visit| {
        // Without the process-wide ids of chevron paths.
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
fn tree_nodes_from_blocks_draw_like_written_ones() -> Result {
    let fonts = fonts()?;
    let blocks = ui(&fonts)?;
    build(
        &blocks,
        r#"TreeView {
            state names: list<string> = ["甲", "乙"]
            TreeNode { text: "根"; expanded: true
                for name in names { TreeNode { text: name; expanded: true
                    if name == "甲" { TreeNode { text: "子" } }
                } }
            }
        }"#,
    )?;
    let written = ui(&fonts)?;
    build(
        &written,
        r#"TreeView {
            TreeNode { text: "根"; expanded: true
                TreeNode { text: "甲"; expanded: true; TreeNode { text: "子" } }
                TreeNode { text: "乙"; expanded: true }
            }
        }"#,
    )?;
    // Same depths, chevrons and positions.
    assert_eq!(scenes(&blocks)?, scenes(&written)?);
    Ok(())
}
