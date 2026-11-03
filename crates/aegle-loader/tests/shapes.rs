//! Shape and Arc elements: SVG path data, fills, strokes and a bound sweep.

use aegle_loader::{Data, Elements, Program};
use aegle_scene::{Command, Point};
use aegle_ui::{Result, Visit};

mod common;

/// Every path drawn, as (filled, last point in window coordinates).
fn drawn(ui: &aegle_ui::Ui) -> Result<Vec<(bool, Point)>> {
    ui.refresh()?;
    let mut paths = Vec::new();
    ui.visit_scenes(|visit| {
        if let Visit::Scene {
            scene, transform, ..
        } = visit
        {
            for command in scene.commands() {
                let (index, filled) = match *command {
                    Command::FillPath { path, .. } => (path, true),
                    Command::StrokePath { path, .. } => (path, false),
                    _ => continue,
                };
                let last = *scene.paths()[index].points().last().unwrap();
                paths.push((filled, transform.map_point(last)));
            }
        }
        Ok(())
    })?;
    Ok(paths)
}

#[test]
fn shapes_draw_svg_paths_and_arcs_follow_their_sweep() -> Result {
    let ui = common::ui()?;
    ui.root().set_padding(0.0);
    let source = r#"Column { gap: 0dp
        state sweep: float = 90.0
        state data: string = "M 0 0 L 20 0 L 20 10 Z"
        Shape { width: 20dp; height: 10dp; path: data; fill: #ff0000; stroke: #000000 }
        Arc { width: 40dp; height: 40dp; sweep: sweep; stroke: #00ff00
            stroke_width: 4dp; cap: round }
        Column { id: tab; radius: 8dp; corners: [bottom_left, bottom_right] }
    }"#;
    let program =
        Program::from_sources("main.aegle", &Elements::new(), &mut |_| Ok(source.into()))?;
    let view = program.build(&ui.root())?;
    let corners = view.handle("tab").unwrap().node().corners();
    assert!(corners.bottom_left && corners.bottom_right && !corners.top_left && !corners.top_right);
    let near = |a: Point, b: Point| (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3;
    let paths = drawn(&ui)?;
    // The triangle is filled and stroked; the arc only stroked.
    assert_eq!(paths.len(), 3);
    assert!(paths[0].0 && !paths[1].0 && !paths[2].0);
    // A quarter turn from the top of the circle of radius 18 around (20, 30)
    // ends at its right.
    assert!(near(paths[2].1, Point::new(38.0, 30.0)), "{:?}", paths[2].1);
    view.set("sweep", Data::Float(180.0))?;
    assert!(near(drawn(&ui)?[2].1, Point::new(20.0, 48.0)));
    // Data that does not parse draws nothing.
    view.set("data", Data::String("M 0".into()))?;
    assert_eq!(drawn(&ui)?.len(), 1);
    Ok(())
}
