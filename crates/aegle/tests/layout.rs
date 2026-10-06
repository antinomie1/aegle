//! Compiled and runtime-loaded markup produce the same advanced layout, and
//! generated `for` rows take part in their grid like ordinary children.
#![cfg(all(feature = "markup", feature = "grid"))]

use aegle::{
    Result, Size, TextSystem, Theme, Ui,
    loader::{Data, Program},
    scene::Rect,
};
use std::{cell::RefCell, rc::Rc};

const IDS: [&str; 14] = [
    "bar", "lead", "trail", "flow", "one", "two", "three", "span", "corner", "stack", "badge",
    "mirrored", "first", "overlay",
];

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    ui.resize(Size::new(400.0, 300.0))?;
    Ok(ui)
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(x, y, width, height)
}

#[test]
fn compiled_and_loaded_layouts_match() -> Result {
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/layout.aegle")?;
    compiled.refresh()?;
    let expected = [
        (&view.lead, rect(8.0, 19.0, 76.8, 10.0)),
        (&view.trail, rect(358.0, 24.0, 30.0, 20.0)),
        (&view.one, rect(8.0, 50.0, 40.0, 10.0)),
        (&view.two, rect(52.0, 50.0, 40.0, 10.0)),
        (&view.three, rect(8.0, 62.0, 30.0, 10.0)),
        (&view.span, rect(58.0, 78.0, 334.0, 12.0)),
        (&view.corner, rect(382.0, 90.0, 10.0, 12.0)),
        (&view.badge, rect(196.0, 119.0, 8.0, 8.0)),
        (&view.first, rect(372.0, 144.0, 20.0, 10.0)),
        (&view.overlay, rect(384.0, 284.0, 16.0, 16.0)),
    ];
    for (node, bounds) in expected {
        assert_eq!(node.bounds()?, bounds);
    }

    let loaded = ui()?;
    let program = Program::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/layout.aegle"
    ))?;
    let runtime = program.build(&loaded.root())?;
    loaded.refresh()?;
    let compiled_bounds = [
        &*view.bar,
        &view.lead,
        &view.trail,
        &view.flow,
        &view.one,
        &view.two,
        &view.three,
        &view.span,
        &view.corner,
        &view.stack,
        &view.badge,
        &*view.mirrored,
        &view.first,
        &view.overlay,
    ];
    assert_eq!(runtime.root().node().bounds()?, view.root.bounds()?);
    for (id, node) in IDS.iter().zip(compiled_bounds) {
        let handle = runtime.handle(id).unwrap();
        assert_eq!(handle.node().bounds()?, node.bounds()?, "{id}");
    }
    Ok(())
}

#[test]
fn generated_rows_fill_grid_cells() -> Result {
    let ui = ui()?;
    let source = r#"
        Grid {
            state names: list<string> = ["a", "b", "c"]
            columns: [100dp, 100dp]
            auto_rows: 20dp
            gap: 0dp
            Column { id: head; height: 20dp; grid_column: [1, 2] }
            for name in names { Column { height: 20dp } }
        }
    "#;
    let program = Program::from_sources("grid.aegle", &mut |_| Ok(source.to_owned()))?;
    let view = program.build(&ui.root())?;
    ui.refresh()?;
    // Header row plus two rows of two cells; one wrapper cell would stack them instead.
    let grid = view.root().node();
    assert_eq!(grid.bounds()?.size.height, 60.0);
    view.set("names", Data::List(vec![Data::String("z".into())].into()))?;
    ui.refresh()?;
    assert_eq!(grid.bounds()?.size.height, 40.0);
    assert_eq!(
        view.handle("head").unwrap().node().bounds()?.size.width,
        200.0
    );
    Ok(())
}
