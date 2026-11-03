//! A program fills a node's background in place of its color; only one that
//! names `time` keeps frames coming, and reduced motion stops it.

use std::{cell::RefCell, rc::Rc, time::Instant};

use aegle_ui::{
    Color, Result, Size, TextSystem, Theme, Ui,
    scene::{Command, Shader},
};

const STILL: &str =
    "fn shade(uv: vec2f, size: vec2f) -> vec4f { return vec4f(uv, values[0], 1.0); }";
const MOVING: &str =
    "fn shade(uv: vec2f, size: vec2f) -> vec4f { return vec4f(fract(time), 0.0, 0.0, 1.0); }";

/// The node's own commands: its scene draws first, so the first one found
/// with a program or a fill.
fn background(ui: &Ui) -> Result<Option<Command>> {
    ui.refresh()?;
    let mut found = None;
    ui.visit_scenes(|visit| {
        if let aegle_ui::Visit::Scene { scene, .. } = visit {
            for command in scene.commands() {
                if matches!(command, Command::Shader { .. } | Command::Fill { .. }) {
                    found = found.or(Some(*command));
                }
            }
        }
        Ok(())
    })?;
    Ok(found)
}

#[test]
fn programs_fill_backgrounds_and_frames_follow_time() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0));
    ui.root().set_background(Color::TRANSPARENT);
    let panel = ui.root().column();
    panel.set_width(80.0);
    panel.set_height(40.0);
    panel.set_background(Color::rgb(10, 20, 30));
    panel.set_shader_values(&[0.25, 1.0]);
    panel.set_shader(Shader::new(STILL));
    let Some(Command::Shader { values, time, .. }) = background(&ui)? else {
        panic!("the program replaces the color");
    };
    assert_eq!((&values[..3], time), (&[0.25, 1.0, 0.0][..], 0.0));
    assert!(!ui.wants_frames(), "a program without time draws once");

    panel.set_shader(Shader::new(MOVING));
    ui.refresh()?;
    assert!(ui.wants_frames(), "time keeps frames coming");
    ui.run_frame(Instant::now() + std::time::Duration::from_millis(1500));
    let Some(Command::Shader { values, time, .. }) = background(&ui)? else {
        panic!("still shaded");
    };
    assert!(time >= 1.5, "{time}");
    assert_eq!(values[0], 0.25, "values outlive a change of program");

    // The host's next frame draws it held.
    ui.set_reduced_motion(true);
    ui.run_frame(Instant::now());
    let Some(Command::Shader { time, .. }) = background(&ui)? else {
        panic!("still shaded");
    };
    assert_eq!(time, 0.0);
    assert!(!ui.wants_frames(), "reduced motion holds time");
    ui.set_reduced_motion(false);

    panel.set_visible(false);
    ui.refresh()?;
    assert!(!ui.wants_frames(), "hidden programs draw nothing");
    panel.set_visible(true);

    panel.set_shader(None);
    assert!(matches!(background(&ui)?, Some(Command::Fill { .. })));
    assert!(!ui.wants_frames());
    let too_many = std::panic::AssertUnwindSafe(|| panel.set_shader_values(&[0.0; 9]));
    assert!(std::panic::catch_unwind(too_many).is_err());
    Ok(())
}
