//! Boundary and retarget behavior using the same linear-light color math as rendering.
use aegle_motion::{Animation, Cycles, Duration, Easing, Keyframe, Spring, Transition, Tween};
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

#[test]
fn custom_curves_match_css_and_springs_settle() {
    // CSS `ease` passes about 0.8024 at half time.
    let ease = Easing::cubic_bezier(0.25, 0.1, 0.25, 1.0).unwrap();
    assert!((ease.sample(0.5) - 0.8024).abs() < 1e-3);
    assert_eq!((ease.sample(0.0), ease.sample(1.0)), (0.0, 1.0));
    assert!(Easing::cubic_bezier(1.5, 0.0, 0.5, 1.0).is_err());
    assert!(Easing::cubic_bezier(0.5, f32::NAN, 0.5, 1.0).is_err());

    let peak = |spring: Spring| {
        let timing = Transition::spring(spring);
        let tween = Tween::new(0.0f32, 1.0, timing.duration, timing.easing).unwrap();
        let steps = (0..=200).map(|i| tween.sample(timing.duration * i / 200));
        let peak = steps.fold(0.0f32, f32::max);
        let near_end = tween.sample(timing.duration.mul_f32(0.999));
        (peak, near_end)
    };
    let (bouncy, end) = peak(Spring::new(300.0, 10.0).unwrap());
    assert!(bouncy > 1.05, "an underdamped spring overshoots");
    assert!((end - 1.0).abs() < 0.01, "and settles by its duration");
    let (critical, end) = peak(Spring::new(170.0, 2.0 * 170f32.sqrt()).unwrap());
    assert!(critical <= 1.0 && (end - 1.0).abs() < 0.01);
    let (creeping, end) = peak(Spring::new(100.0, 60.0).unwrap());
    assert!(creeping <= 1.0 && (end - 1.0).abs() < 0.01);
    assert!(Spring::new(0.0, 1.0).is_err() && Spring::new(1.0, -1.0).is_err());
}

#[test]
fn keyframes_delay_cycles_and_alternate() {
    let ms = Duration::from_millis;
    let frames = [
        Keyframe::new(0.0, 0.0f32),
        Keyframe::new(0.25, 10.0),
        Keyframe::new(1.0, 0.0).easing(Easing::EaseIn),
    ];
    let hop = Animation::new(ms(100), frames).unwrap().delay(ms(50));
    assert_eq!(
        hop.sample(ms(20)),
        0.0,
        "holds the first value while delayed"
    );
    assert_eq!(hop.sample(ms(75)), 10.0);
    let at = |position: f64| hop.sample(ms(50) + ms(100).mul_f64(position));
    assert_eq!(at(0.125), 5.0, "linear into the second frame");
    assert_eq!(at(0.625), 7.5, "ease-in leaves the peak slowly");
    assert!(hop.finished(ms(150)) && hop.sample(ms(150)) == 0.0);

    let swing = Animation::tween(0.0f32, 1.0, Transition::new(ms(100), Easing::Linear))
        .unwrap()
        .cycles(Cycles::Times(2))
        .alternate(true);
    assert_eq!(swing.sample(ms(150)), 0.5);
    assert_eq!(
        swing.sample(ms(175)),
        0.25,
        "the second cycle runs backwards"
    );
    assert_eq!(
        swing.target(),
        0.0,
        "an even alternate count ends at the start"
    );
    let forever = swing.clone().cycles(Cycles::Forever);
    assert!(!forever.finished(Duration::MAX));
    assert_eq!(forever.target(), 1.0);

    let bad = [Keyframe::new(0.0, 0.0f32), Keyframe::new(0.5, 1.0)];
    assert!(Animation::new(ms(100), bad).is_err(), "must end at 1.0");
    let backwards = [
        Keyframe::new(0.0, 0.0f32),
        Keyframe::new(0.6, 1.0),
        Keyframe::new(0.4, 1.0),
        Keyframe::new(1.0, 1.0),
    ];
    assert!(Animation::new(ms(100), backwards).is_err());
}
