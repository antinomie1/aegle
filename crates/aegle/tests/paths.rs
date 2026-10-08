//! `ui!` and the runtime engine build the same interface from one document:
//! every identifier choice maps to the same value, and properties apply in
//! the same order, so an ancestor's initial value never starts a transition.
#![cfg(all(feature = "markup", feature = "grid", feature = "motion"))]

use aegle::{
    Node, Result, Size, TextSystem, Theme, TransitionProperty, Ui,
    loader::{
        Program,
        markup::{CheckedNode, PropertyName, Value as Literal, check, choices, parse},
    },
};
use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    ui.resize(Size::new(640.0, 480.0))?;
    Ok(ui)
}

fn load(ui: &Ui, fixture: &str) -> Result<aegle::loader::View> {
    let path = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    Program::load(path)?.build(&ui.root())
}

/// Every painted record with its placement and clip, and every layer, in paint order.
fn scenes(ui: &Ui) -> Result<Vec<String>> {
    ui.refresh()?;
    let mut scenes = Vec::new();
    ui.visit_scenes(|visit| {
        scenes.push(format!("{visit:?}"));
        Ok(())
    })?;
    Ok(scenes)
}

fn timings(node: &Node) -> Result<[Option<aegle::Transition>; 2]> {
    Ok([
        node.property_transition(TransitionProperty::Paint)?,
        node.property_transition(TransitionProperty::Offset)?,
    ])
}

#[test]
fn every_choice_builds_alike() -> Result {
    let source = include_str!("fixtures/choices.aegle");
    let document = check(parse(source)?)?;
    let mut used = BTreeSet::new();
    fn visit(node: &CheckedNode, used: &mut BTreeSet<(String, String)>) {
        for property in &node.properties {
            if let Literal::Identifier(value) = &property.value
                && !choices(property.name).is_empty()
            {
                used.insert((format!("{:?}", property.name), value.clone()));
            }
        }
        node.children.iter().for_each(|child| visit(child, used));
    }
    visit(&document.root, &mut used);
    // `theme` applies only to windows, which need a native host.
    use PropertyName::*;
    for name in [
        Direction,
        LayoutDirection,
        Wrap,
        Align,
        AlignSelf,
        Justify,
        AlignContent,
        JustifySelf,
        JustifyItems,
        Flow,
        Easing,
        Orientation,
    ] {
        for choice in choices(name) {
            let key = (format!("{name:?}"), choice.to_string());
            assert!(used.contains(&key), "fixture lacks {key:?}");
        }
    }

    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/choices.aegle")?;
    let loaded = ui()?;
    let runtime = load(&loaded, "choices.aegle")?;
    assert_eq!(scenes(&compiled)?, scenes(&loaded)?);
    for (node, id) in [
        (&view.linear, "linear"),
        (&view.ease_in, "ease_in"),
        (&view.ease_out, "ease_out"),
        (&view.ease_in_out, "ease_in_out"),
    ] {
        let loaded = runtime.handle(id).unwrap().node();
        assert_eq!(timings(node)?, timings(loaded)?, "{id}");
    }
    Ok(())
}

#[test]
fn ancestors_apply_before_transitions_install() -> Result {
    let compiled = ui()?;
    let view = aegle::ui!(compiled.root(), "tests/fixtures/order.aegle")?;
    let loaded = ui()?;
    let runtime = load(&loaded, "order.aegle")?;
    let slider = runtime.handle("slider").unwrap().node();
    assert_eq!(scenes(&compiled)?, scenes(&loaded)?);
    assert!(!view.slider.is_animating()?);
    assert!(!slider.is_animating()?);
    Ok(())
}
