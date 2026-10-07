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

#[test]
fn shared_images_and_validated_paths() {
    use aegle_scene::{FillRule, Image, PathBuilder, Stroke};
    assert_eq!(
        Image::new(0, 1, vec![]).unwrap_err(),
        SceneError::InvalidImage
    );
    assert_eq!(
        Image::new(1, 1, vec![0; 3]).unwrap_err(),
        SceneError::InvalidImage
    );
    let image = Image::new(1, 1, vec![0; 4]).unwrap();
    assert_eq!(image.clone().id(), image.id());
    let mut open = PathBuilder::new();
    open.line_to(Point::new(1.0, 1.0));
    assert_eq!(
        open.finish(FillRule::NonZero).unwrap_err(),
        SceneError::InvalidPath
    );
    let mut closed = PathBuilder::new();
    closed
        .move_to(Point::new(0.0, 0.0))
        .close()
        .line_to(Point::new(1.0, 0.0));
    assert_eq!(
        closed.finish(FillRule::NonZero).unwrap_err(),
        SceneError::InvalidPath
    );
    let mut curve = PathBuilder::new();
    curve
        .move_to(Point::new(2.0, 1.0))
        .quad_to(Point::new(8.0, -3.0), Point::new(4.0, 6.0));
    let curve = curve.finish(FillRule::EvenOdd).unwrap();
    assert_eq!(curve.bounds(), Rect::new(2.0, -3.0, 6.0, 9.0));
    let mut point = PathBuilder::new();
    point.move_to(Point::new(1.0, 1.0));
    let point = point.finish(FillRule::NonZero).unwrap();
    assert!(point.is_empty());

    let mut builder = SceneBuilder::new();
    builder
        .image(&image, Rect::new(0.0, 0.0, 0.0, 4.0))
        .unwrap()
        .fill_path(&point, Color::BLACK)
        .unwrap()
        .stroke_path(&curve, Color::BLACK, Stroke::new(0.0))
        .unwrap();
    assert_eq!(
        builder
            .stroke_path(&curve, Color::BLACK, Stroke::new(-1.0))
            .unwrap_err(),
        SceneError::NegativeExtent
    );
    builder
        .image(&image, Rect::new(0.0, 0.0, 4.0, 4.0))
        .unwrap()
        .fill_path(&curve, Color::BLACK)
        .unwrap()
        .stroke_path(&curve, Color::WHITE, Stroke::new(2.0))
        .unwrap();
    let scene = builder.finish().unwrap();
    assert_eq!(scene.len(), 3);
    assert_eq!((scene.images().len(), scene.paths().len()), (1, 2));
}

#[test]
fn validated_gradients_and_shadows() {
    use aegle_scene::{Gradient, GradientStop};
    let stop = |offset| GradientStop {
        offset,
        color: Color::BLACK,
    };
    let (a, b) = (Point::new(0.0, 0.0), Point::new(10.0, 0.0));
    for stops in [
        &[stop(0.0)][..],
        &[stop(0.6), stop(0.4)],
        &[stop(0.0), stop(1.5)],
    ] {
        assert_eq!(
            Gradient::linear(a, b, stops).unwrap_err(),
            SceneError::InvalidGradient
        );
    }
    let stops = [stop(0.0), stop(0.5), stop(0.5), stop(1.0)];
    assert!(Gradient::linear(a, a, &stops).is_err());
    assert!(Gradient::radial(a, 0.0, &stops).is_err());
    assert_eq!(
        Gradient::radial(a, f32::NAN, &stops).unwrap_err(),
        SceneError::NonFinite
    );
    let gradient = Gradient::linear(a, b, &stops).unwrap();
    let shape = RoundedRect::new(Rect::new(0.0, 0.0, 8.0, 8.0), 2.0).unwrap();
    let mut builder = SceneBuilder::new();
    assert_eq!(
        builder.shadow(shape, Color::BLACK, -1.0).unwrap_err(),
        SceneError::NegativeExtent
    );
    builder
        .fill_gradient(shape, &gradient)
        .unwrap()
        .shadow(shape, Color::BLACK, 0.0)
        .unwrap()
        .shadow(shape, Color::BLACK, 3.0)
        .unwrap();
    let scene = builder.finish().unwrap();
    assert_eq!(scene.gradients(), [gradient]);
    assert!(matches!(
        scene.commands(),
        [
            Command::FillGradient { gradient: 0, .. },
            Command::Fill { .. },
            Command::Shadow { blur: 3.0, .. }
        ]
    ));
}

#[test]
fn bounds_cover_drawn_pixels_within_clips() -> Result<(), SceneError> {
    let shape = |x, y, w, h| RoundedRect::new(Rect::new(x, y, w, h), 0.0);
    assert_eq!(SceneBuilder::new().finish()?.bounds(), None);
    let mut builder = SceneBuilder::new();
    builder.fill(shape(0.0, 0.0, 10.0, 10.0)?, Color::BLACK)?;
    builder.push_transform(Affine::translation(20.0, 0.0)?.then(Affine::scale(2.0, 2.0)?)?)?;
    builder.stroke(shape(0.0, 0.0, 5.0, 5.0)?, Color::BLACK, 2.0)?;
    builder.pop()?;
    builder.shadow(shape(0.0, 30.0, 10.0, 10.0)?, Color::BLACK, 2.0)?;
    // Clipped drawing counts only inside its clip; an empty clip hides all.
    builder.push_clip(shape(-5.0, -5.0, 2.0, 2.0)?)?;
    builder.fill(shape(-100.0, -100.0, 200.0, 200.0)?, Color::BLACK)?;
    builder.pop()?;
    builder.push_clip(shape(0.0, 0.0, 0.0, 0.0)?)?;
    builder.fill(shape(500.0, 500.0, 10.0, 10.0)?, Color::BLACK)?;
    builder.pop()?;
    // Translated then scaled, the stroke reaches x 38..52; the shadow reaches
    // 6 beyond its shape and the clipped fill up to y -5.
    assert_eq!(
        builder.finish()?.bounds(),
        Some(Rect::new(-6.0, -5.0, 58.0, 51.0))
    );
    Ok(())
}
