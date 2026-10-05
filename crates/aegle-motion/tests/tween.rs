//! Boundary and retarget behavior using the same linear-light color math as rendering.
use aegle_motion::{Duration, Easing, Transition, Tween};
use aegle_types::{Color, Point};

#[test]
fn elapsed_sampling_retargets_without_overflow_or_transparent_color_fringes() {
    let duration = Duration::from_millis(100);
    let half = duration / 2;
    let tween = Tween::new(-f32::MAX, f32::MAX, duration, Easing::Linear).unwrap();
    assert_eq!(tween.sample(Duration::ZERO), -f32::MAX);
    assert_eq!(tween.sample(half), 0.0);
    assert_eq!(tween.sample(Duration::MAX), f32::MAX);
    assert!(!tween.finished(half));
    assert!(tween.finished(duration));
    assert!(Tween::new(f32::NAN, 1.0, duration, Easing::Linear).is_err());
    assert!(
        Tween::new(
            Point::default(),
            Point::new(0.0, f32::INFINITY),
            duration,
            Easing::Linear
        )
        .is_err()
    );
    let zero = Tween::new(1.0, 2.0, Duration::ZERO, Easing::EaseOut).unwrap();
    assert_eq!(zero.sample(Duration::ZERO), 2.0);
    assert!(zero.finished(Duration::ZERO));

    for (easing, expected) in [
        (Easing::Linear, 0.5),
        (Easing::EaseIn, 0.25),
        (Easing::EaseOut, 0.75),
        (Easing::EaseInOut, 0.5),
    ] {
        let tween = Tween::new(0.0, 1.0, duration, easing).unwrap();
        assert_eq!(tween.sample(half), expected);
    }
    let point = Tween::new(
        Point::default(),
        Point::new(8.0, -8.0),
        duration,
        Easing::EaseInOut,
    )
    .unwrap();
    assert_eq!(point.sample(duration / 4), Point::new(1.0, -1.0));
    let retarget = Tween::new(
        point.sample(half),
        Point::default(),
        duration,
        Easing::EaseOut,
    )
    .unwrap();
    assert_eq!(retarget.sample(Duration::ZERO), point.sample(half));
    assert_eq!(retarget.sample(half), Point::new(1.0, -1.0));

    let colors = Tween::new(Color::BLACK, Color::WHITE, duration, Easing::Linear).unwrap();
    assert_eq!(colors.sample(half).to_rgba(), [188, 188, 188, 255]);
    let transparent_red = Color::rgba(255, 0, 0, 0);
    let constant = Tween::new(transparent_red, transparent_red, duration, Easing::Linear).unwrap();
    assert_eq!(constant.sample(half), transparent_red);
    let blue = Color::rgb(0, 0, 255);
    let colors = Tween::new(transparent_red, blue, duration, Easing::Linear).unwrap();
    assert_eq!(colors.sample(Duration::ZERO), transparent_red);
    assert_eq!(colors.sample(half).to_rgba(), [0, 0, 255, 128]);
    assert_eq!(colors.sample(duration), blue);
    let timing = Transition::default();
    assert_eq!(
        timing,
        Transition::new(Duration::from_millis(120), Easing::EaseOut)
    );
}
