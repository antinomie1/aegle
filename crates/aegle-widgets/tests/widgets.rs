//! Painters emit the expected scene commands and scroll geometry is consistent.
use aegle_scene::{Command, SceneBuilder, SceneError};
use aegle_theme::Theme;
use aegle_types::Size;
use aegle_widgets::{Mark, ToggleSpec, slider_track, toggle};

fn appearance() -> aegle_theme::Appearance {
    aegle_theme::Appearance::new(
        &Theme::default(),
        aegle_theme::VisualState {
            kind: aegle_theme::ControlKind::CheckBox,
            enabled: true,
            hovered: false,
            pressed: false,
            focused: false,
            checked: true,
            read_only: false,
        },
    )
}

#[test]
fn toggles_paint_markers_and_call_the_label_only_when_present() -> Result<(), SceneError> {
    let spec = |mark, mixed, label_height| ToggleSpec {
        size: Size::new(120.0, 24.0),
        padding: 3.0,
        gap: 8.0,
        mark,
        checked: true,
        mixed,
        label_height,
    };
    let commands = |spec: &ToggleSpec| -> Result<(usize, bool), SceneError> {
        let mut builder = SceneBuilder::new();
        let mut called = false;
        toggle(&mut builder, spec, appearance(), |_, color| {
            called = color == appearance().foreground;
            Ok::<_, SceneError>(())
        })?;
        Ok((builder.finish()?.commands().len(), called))
    };
    let (plain, called) = commands(&spec(Mark::Check, false, None))?;
    assert!(!called, "no label height, no label");
    let (labeled, called) = commands(&spec(Mark::Check, false, Some(14.0)))?;
    assert!(called);
    assert_eq!(
        labeled,
        plain + 2,
        "the label adds only a transform push and pop"
    );
    // A mixed box draws a bar where a checked one draws two rotated strokes.
    let (mixed, _) = commands(&spec(Mark::Check, true, None))?;
    let (checked, _) = commands(&spec(Mark::Check, false, None))?;
    assert!(mixed < checked);
    let (switch, _) = commands(&spec(Mark::Switch, false, None))?;
    assert!(switch > 0);
    let mut builder = SceneBuilder::new();
    toggle(
        &mut builder,
        &spec(Mark::Radio, false, None),
        appearance(),
        |_, _| Ok::<_, SceneError>(()),
    )?;
    let scene = builder.finish()?;
    assert!(
        matches!(scene.commands()[0], Command::PushClip(_)),
        "painters clip to their bounds"
    );
    Ok(())
}

#[test]
fn the_slider_track_stays_inside_tiny_controls() {
    let (start, length) = slider_track(Size::new(10.0, 10.0), 8.0);
    assert!(start >= 0.0 && start + length <= 10.0);
}
