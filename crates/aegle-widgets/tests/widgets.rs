//! Painters emit the expected scene commands and scroll geometry is consistent.
use aegle_scene::{Color, Command, Point, Rect, SceneBuilder, SceneError};
use aegle_theme::Theme;
use aegle_types::Size;
use aegle_widgets::{
    Bar, Mark, ToggleSpec, clamp_anchor, intersection, reveal_delta, scrollbar, slider_track,
    toggle,
};

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
fn scrollbars_follow_the_offset_and_map_drags_back_to_fractions() {
    let size = Size::new(200.0, 100.0);
    // 300 more pixels vertically and nothing horizontally.
    let [vertical, horizontal] =
        Bar::layout(size, Point::new(0.0, 150.0), Point::new(0.0, 300.0), true);
    assert!(horizontal.is_none());
    let bar = vertical.unwrap();
    assert!(bar.vertical && bar.length >= 24.0 && bar.length < bar.strip.size.height);
    assert!(
        (bar.thumb - bar.travel() * 0.5).abs() < 1e-3,
        "halfway scrolled, halfway down"
    );
    assert!(bar.strip.origin.x + bar.strip.size.width <= size.width);

    // A press on the thumb keeps the grab point; elsewhere it centers the thumb.
    let on_thumb = bar.thumb + 3.0;
    assert_eq!(bar.grab(on_thumb), 3.0);
    assert_eq!(bar.grab(bar.thumb + bar.length + 5.0), bar.length * 0.5);
    let half = bar
        .fraction(bar.thumb + bar.grab(on_thumb), bar.grab(on_thumb))
        .unwrap();
    assert!((half - 0.5).abs() < 1e-3);
    assert_eq!(
        bar.fraction(-1000.0, 0.0),
        Some(0.0),
        "dragging past the end clamps"
    );

    // Both axes leave a corner, and an editor never gets a horizontal bar.
    let [v, h] = Bar::layout(size, Point::new(0.0, 0.0), Point::new(50.0, 50.0), true);
    assert!(v.unwrap().strip.size.height < size.height - scrollbar::STRIP + 1.0);
    assert!(h.is_some());
    assert!(Bar::layout(size, Point::new(0.0, 0.0), Point::new(50.0, 50.0), false)[1].is_none());

    let mut builder = SceneBuilder::new();
    scrollbar::paint(
        &mut builder,
        [Some(bar), None],
        [Color::WHITE, Color::BLACK],
        4.0,
    )
    .unwrap();
    assert_eq!(
        builder.finish().unwrap().commands().len(),
        2,
        "one track and one thumb"
    );
}

#[test]
fn scroll_helpers_and_slider_track_stay_inside_their_bounds() {
    assert_eq!(
        reveal_delta(5.0, 10.0, 20.0, 100.0),
        -15.0,
        "above scrolls up"
    );
    assert_eq!(
        reveal_delta(110.0, 20.0, 20.0, 100.0),
        10.0,
        "below scrolls down"
    );
    assert_eq!(
        reveal_delta(30.0, 10.0, 20.0, 100.0),
        0.0,
        "visible stays put"
    );
    let overlap = intersection(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(6.0, 8.0, 10.0, 10.0),
    );
    assert_eq!(overlap, Rect::new(6.0, 8.0, 4.0, 2.0));
    let apart = intersection(
        Rect::new(0.0, 0.0, 10.0, 10.0),
        Rect::new(30.0, 30.0, 5.0, 5.0),
    );
    assert!(apart.is_empty());
    let anchor = clamp_anchor(
        Rect::new(500.0, 3.0, 2.0, 12.0),
        Rect::new(0.0, 0.0, 100.0, 50.0),
    );
    assert_eq!(
        (anchor.origin.x, anchor.size.width),
        (100.0, 0.0),
        "a clipped caret sticks to the edge"
    );
    let (start, length) = slider_track(Size::new(10.0, 10.0), 8.0);
    assert!(
        start >= 0.0 && start + length <= 10.0,
        "the thumb stays inside tiny controls"
    );
}
