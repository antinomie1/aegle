//! Programs know whether they read `time`, and scenes share them.

use aegle_scene::{Command, Rect, RoundedRect, SceneBuilder, Shader};

#[test]
fn programs_that_name_time_are_animated() {
    let still = "// time stands still\nfn shade(uv: vec2f, s: vec2f) -> vec4f { /* time */ return vec4f(uv, 0.0, 1.0); }";
    let moving = "fn shade(uv: vec2f, s: vec2f) -> vec4f { return vec4f(uv, fract(time), 1.0); }";
    let lookalike =
        "fn shade(uv: vec2f, s: vec2f) -> vec4f { let times = 2.0; return vec4f(uv, times, 1.0); }";
    assert!(!Shader::new(still).is_animated());
    assert!(Shader::new(moving).is_animated());
    assert!(!Shader::new(lookalike).is_animated());
    assert_eq!(Shader::new(moving).key(), Shader::new(moving).key());
    assert_ne!(Shader::new(moving).key(), Shader::new(still).key());
}

#[test]
fn a_scene_lists_each_program_once() {
    let shader = Shader::new("fn shade(uv: vec2f, s: vec2f) -> vec4f { return vec4f(1.0); }");
    let shape = RoundedRect::new(Rect::new(0.0, 0.0, 10.0, 10.0), 2.0);
    let mut builder = SceneBuilder::new();
    builder.shade(shape, &shader, 1.5, [0.0; Shader::VALUES]);
    builder.shade(shape, &shader.clone(), 2.0, [1.0; Shader::VALUES]);
    let scene = builder.finish();
    assert_eq!(scene.shaders().len(), 1);
    assert!(matches!(
        scene.commands()[1],
        Command::Shader {
            shader: 0,
            time: 2.0,
            ..
        }
    ));
}
