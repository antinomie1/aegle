//! Scrollbar geometry and scroll helpers stay consistent and inside their bounds.
use aegle_scene::{Color, Point, Rect, SceneBuilder};
use aegle_types::Size;
use aegle_ui::{
    bar::{self, Bar},
    scroll_geometry::{clamp_anchor, intersection, reveal_delta},
};

#[test]
fn scrollbars_follow_the_offset_and_map_drags_back_to_fractions() {
    let size = Size::new(200.0, 100.0);
    // 300 more pixels vertically and nothing horizontally.
    let [vertical, horizontal] = Bar::layout(
        size,
        Point::new(0.0, 150.0),
        Point::new(0.0, 300.0),
        true,
        false,
        0.0,
        false,
    );
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
    let limit = Point::new(50.0, 50.0);
    let [v, h] = Bar::layout(size, Point::new(0.0, 0.0), limit, true, false, 0.0, false);
    assert!(v.unwrap().strip.size.height < size.height - bar::STRIP + 1.0);
    assert!(h.is_some());
    assert!(Bar::layout(size, Point::new(0.0, 0.0), limit, false, false, 0.0, false)[1].is_none());

    // Right to left the vertical bar and the corner move to the left edge, and
    // an unscrolled horizontal thumb rests at the right (start) end.
    let [v, h] = Bar::layout(size, Point::new(0.0, 0.0), limit, true, true, 0.0, false);
    let (v, h) = (v.unwrap(), h.unwrap());
    assert_eq!(v.strip.origin.x, 0.0);
    assert!(v.thumb_rect().origin.x < bar::STRIP);
    assert!(h.strip.origin.x >= bar::STRIP);
    assert!(h.strip.origin.x + h.strip.size.width <= size.width);
    assert_eq!(h.thumb, h.travel());
    let start = h.fraction(h.thumb + 1.0, 1.0).unwrap();
    assert_eq!(start, 0.0, "the right end is offset zero");

    let mut builder = SceneBuilder::new();
    bar::paint(
        &mut builder,
        [Some(bar), None],
        [Color::WHITE, Color::BLACK],
    )
    .unwrap();
    assert_eq!(
        builder.finish().unwrap().commands().len(),
        2,
        "one track and one thumb"
    );

    // A rounded viewport keeps the bar's ends clear of its corners, and the
    // bar is thin at rest and full thickness while active.
    let rounded = |active| {
        Bar::layout(
            size,
            Point::new(0.0, 0.0),
            Point::new(0.0, 300.0),
            true,
            false,
            24.0,
            active,
        )[0]
        .unwrap()
    };
    let (rest, active) = (rounded(false), rounded(true));
    assert!(rest.strip.origin.y > 10.0, "below the top corner's curve");
    assert!(rest.strip.origin.y + rest.strip.size.height < size.height - 10.0);
    assert_eq!(rest.thumb_rect().size.width, 4.0);
    assert_eq!(active.thumb_rect().size.width, bar::THICKNESS);
    assert_eq!(
        rest.strip, active.strip,
        "hovering doesn't move the hit strip"
    );
}

#[test]
fn scroll_helpers_stay_inside_their_bounds() {
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
}
