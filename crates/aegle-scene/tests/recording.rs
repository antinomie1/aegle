//! Scene boundary and reusable command-buffer integration.

use aegle_scene::{Affine, Color, Command, Point, Rect, RoundedRect, SceneBuilder, SceneError};

#[test]
fn validated_scopes_and_reused_recording() {
    let shape = RoundedRect::new(Rect::new(0.0, 0.0, 40.0, 20.0), 100.0).unwrap();
    assert_eq!(shape.radius(), 10.0);
    assert!(RoundedRect::new(Rect::new(0.0, 0.0, -1.0, 2.0), 0.0).is_err());
    assert!(Affine::scale(0.0, 1.0).is_err());
    assert!(Affine::scale(f32::from_bits(1), 1.0).is_err());
    let translate = Affine::translation(5.0, 7.0).unwrap();
    let scale = Affine::scale(2.0, 3.0).unwrap();
    assert_eq!(
        scale
            .then(translate)
            .unwrap()
            .map_point(Point::new(1.0, 1.0)),
        Point::new(7.0, 10.0)
    );
    let mut builder = SceneBuilder::new();
    assert_eq!(builder.pop().unwrap_err(), SceneError::UnexpectedPop);
    builder
        .push_transform(translate)
        .unwrap()
        .push_clip(shape)
        .unwrap();
    assert_eq!(
        builder.stroke(shape, Color::BLACK, f32::NAN).unwrap_err(),
        SceneError::NonFinite
    );
    builder
        .fill(shape, Color::WHITE)
        .unwrap()
        .pop()
        .unwrap()
        .pop()
        .unwrap();
    let scene = builder.finish().unwrap();
    assert_eq!(scene.max_depth(), 2);
    assert_eq!(scene.max_clip_depth(), 1);
    assert_eq!(scene.len(), 5);
    assert!(matches!(scene.commands()[2], Command::Fill { .. }));
    let allocated = scene.allocated_bytes();
    let mut reused = scene.into_builder();
    reused.clear();
    let huge = RoundedRect::new(Rect::new(0.0, 0.0, f32::MAX, 20.0), 0.0).unwrap();
    reused.push_transform(scale).unwrap();
    assert_eq!(
        reused.fill(huge, Color::BLACK).unwrap_err(),
        SceneError::CoordinateRange
    );
    reused.clear();
    assert_eq!(
        reused.stroke(huge, Color::BLACK, f32::MAX).unwrap_err(),
        SceneError::CoordinateRange
    );
    reused.fill(shape, Color::BLACK).unwrap();
    assert_eq!(reused.finish().unwrap().allocated_bytes(), allocated);
    let mut unclosed = SceneBuilder::new();
    for _ in 0..aegle_scene::MAX_SCOPE_DEPTH {
        unclosed.push_clip(shape).unwrap();
    }
    assert_eq!(
        unclosed.push_clip(shape).unwrap_err(),
        SceneError::ScopeLimit
    );
    assert_eq!(unclosed.finish().unwrap_err(), SceneError::UnclosedScope);
}
