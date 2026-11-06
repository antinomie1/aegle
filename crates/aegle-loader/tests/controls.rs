//! Markup reaches text faces, button sizing, secondary presses and elided lines.

use aegle_loader::{
    Elements, Program,
    markup::{check_program, parse},
};
use aegle_ui::{Modifiers, Point, PointerButton, PointerId, PointerKind, Result};
use aegle_widgets::Button;

mod common;

fn build(ui: &aegle_ui::Ui, source: &str) -> Result<aegle_loader::View> {
    let program =
        Program::from_sources("main.aegle", &Elements::new(), &mut |_| Ok(source.into()))?;
    program.build(&ui.root())
}

#[test]
fn font_family_and_weight_check_their_values() {
    let check = |source: &str| {
        let document = parse(source).unwrap();
        check_program(vec![document], &Elements::new().specs()).map(|_| ())
    };
    assert!(
        check(r#"Text { font_family: "JetBrains Mono, monospace"; font_weight: 700 }"#).is_ok()
    );
    assert!(check(r#"Button { text: "x"; font_family: "monospace"; font_weight: bold }"#).is_ok());
    for bad in [
        r#"Column { font_family: "monospace" }"#,
        r#"Text { font_family: " " }"#,
        "Text { font_weight: 0 }",
        "Text { font_weight: 1.5 }",
        "Text { font_weight: heavy }",
    ] {
        assert!(check(bad).is_err(), "{bad}");
    }
}

#[test]
fn text_faces_merge_family_and_weight() -> Result {
    let ui = common::ui()?;
    let view = build(
        &ui,
        r#"Column { Text { id: t; text: "a"; font_family: "monospace"; font_weight: bold } }"#,
    )?;
    let node = view.handle("t").unwrap().node().clone();
    let font = node.change(|state, id| Ok(state.font(id))).unwrap();
    assert_eq!((font.families, font.weight), ("monospace", 700));
    Ok(())
}

#[test]
fn a_local_minimum_height_lets_a_button_fill_a_stretched_row() -> Result {
    let ui = common::ui()?;
    let view = build(
        &ui,
        r#"Column {
            Row { height: 20dp; align: stretch
                Button { id: fill; text: "1"; min_height: 0dp }
                Button { id: themed; text: "2" }
            }
        }"#,
    )?;
    ui.refresh()?;
    let height = |id: &str| view.handle(id).unwrap().node().bounds().size.height;
    assert_eq!(height("fill"), 20.0);
    assert!(height("themed") > 20.0, "the theme's control height stays");
    Ok(())
}

#[test]
fn a_secondary_press_runs_context_menu() -> Result {
    let ui = common::ui()?;
    let view = build(
        &ui,
        r#"Column {
            state hits: int = 0
            Button { id: b; text: "tag"; on context_menu { hits += 1 } }
        }"#,
    )?;
    ui.refresh()?;
    let button = view.handle("b").unwrap().typed::<Button>().unwrap();
    let b = button.bounds();
    let center = Point::new(
        b.origin.x + b.size.width / 2.0,
        b.origin.y + b.size.height / 2.0,
    );
    let press = PointerKind::ButtonDown(PointerButton::Secondary);
    ui.pointer(PointerId(1), press, center, Modifiers::default())?;
    ui.dispatch_callbacks()?;
    assert_eq!(view.state::<i64>("hits").unwrap().get(), 1);
    Ok(())
}

#[test]
fn a_shrunk_single_line_ends_in_an_ellipsis() -> Result {
    let ui = common::ui()?;
    let view = build(
        &ui,
        r#"Column {
            Row { width: 60dp; clip: true
                Text { id: long; text: "一二三四五六七八九十"; shrink: 1; min_width: 0dp }
            }
            Text { id: short; text: "一" }
        }"#,
    )?;
    ui.refresh()?;
    let label = |id: &str| {
        view.handle(id)
            .unwrap()
            .typed::<aegle_widgets::Label>()
            .unwrap()
    };
    assert!(label("long").is_elided());
    assert!(label("long").bounds().size.width <= 60.0);
    assert_eq!(label("long").text(), "一二三四五六七八九十");
    assert!(!label("short").is_elided());
    Ok(())
}

#[test]
fn an_elided_line_does_not_scroll_its_view_sideways() -> Result {
    let ui = common::ui()?;
    let view = build(
        &ui,
        r#"Column {
            ScrollView { id: list; width: 60dp; height: 40dp
                Text { text: "一二三四五六七八九十"; shrink: 1; min_width: 0dp }
                Column { height: 100dp }
            }
        }"#,
    )?;
    ui.refresh()?;
    let limit = view.handle("list").unwrap().node().max_scroll_offset();
    assert_eq!(limit.x, 0.0);
    assert!(limit.y > 0.0);
    Ok(())
}

#[test]
fn clip_is_a_container_switch() {
    let check = |source: &str| {
        let document = parse(source).unwrap();
        check_program(vec![document], &Elements::new().specs()).map(|_| ())
    };
    assert!(check("Row { clip: true }").is_ok());
    assert!(check(r#"Text { clip: true }"#).is_err());
    assert!(check("Row { clip: 1 }").is_err());
}
