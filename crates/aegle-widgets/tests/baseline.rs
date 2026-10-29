//! Baseline alignment lines up the first text baselines of labels, buttons,
//! editors, number fields and toggles with different fonts, paddings and heights.
use aegle_text::GenericFamily;
use aegle_ui::{Align, Node, Result, TextSystem, Theme, Ui};
use aegle_widgets::*;
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn baseline_rows_align_text_controls() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    let row = ui.root().row();
    row.set_align_items(Some(Align::Baseline));
    let big = row.text("Big");
    big.set_font_size(32.0);
    let button = row.button("Go");
    // Taller than its text: the label is centered, and so is its baseline.
    button.set_width(None);
    button.set_height(Some(60.0));
    let nodes: [Node; 5] = [
        (*big).clone(),
        (*button).clone(),
        (*row.text_field("edit")).clone(),
        (*row.number_field(0.0, 9.0, 1.0)).clone(),
        (*row.check_box("check", false)).clone(),
    ];
    ui.refresh()?;

    let lines: Vec<f32> = nodes
        .iter()
        .map(|node| Ok(node.bounds().origin.y + node.baseline().unwrap()))
        .collect::<Result<_>>()?;
    for line in &lines {
        assert!((line - lines[0]).abs() < 0.01, "{lines:?}");
    }
    let tops: Vec<f32> = nodes
        .iter()
        .map(|node| Ok(node.bounds().origin.y))
        .collect::<Result<_>>()?;
    assert!(tops.iter().any(|&top| top != tops[0]), "{tops:?}");

    // Without baseline alignment the same controls sit at the top instead.
    row.set_align_items(Some(Align::Start));
    ui.refresh()?;
    for node in &nodes {
        assert_eq!(node.bounds().origin.y, row.bounds().origin.y);
    }
    Ok(())
}

#[test]
fn single_line_editors_center_their_text() -> Result {
    let mut fonts = TextSystem::new();
    let families = fonts.register_fonts(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    ))?;
    fonts.set_generic_families(GenericFamily::SansSerif, families.iter().copied());
    let ui = Ui::with_fonts(Rc::new(RefCell::new(fonts)), Theme::light())?;
    ui.root().set_padding(0.0);

    // A taller single-line control moves its text down by half the difference,
    // so the line stays centered, like a button's label.
    let short = ui.root().text_field("edit");
    short.set_height(Some(60.0));
    let tall = ui.root().text_field("edit");
    tall.set_height(Some(100.0));
    ui.refresh()?;
    let (a, b) = (short.baseline().unwrap(), tall.baseline().unwrap());
    assert!((b - a - 20.0).abs() < 0.51, "baselines {a} and {b}");

    // A multiline area keeps its first line at the top padding instead.
    let area = ui.root().text_area("one\ntwo");
    area.set_height(Some(80.0));
    let taller = ui.root().text_area("one\ntwo");
    taller.set_height(Some(120.0));
    ui.refresh()?;
    assert_eq!(area.baseline(), taller.baseline());
    Ok(())
}
