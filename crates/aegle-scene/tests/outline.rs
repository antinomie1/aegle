//! Outlines from SVG path data and arcs.

use aegle_scene::{FillRule, PathBuilder, Point, Verb};

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

#[test]
fn arcs_follow_the_circle_one_cubic_per_quarter() {
    let mut builder = PathBuilder::new();
    let center = Point::new(10.0, 10.0);
    // Three quarters clockwise from the top: top, right, bottom, left.
    let quarter = std::f32::consts::FRAC_PI_2;
    builder.arc(center, 10.0, -quarter, 3.0 * quarter);
    let path = builder.finish(FillRule::NonZero);
    assert_eq!(
        path.verbs(),
        [Verb::Move, Verb::Cubic, Verb::Cubic, Verb::Cubic]
    );
    let points = path.points();
    assert!(close(points[0], Point::new(10.0, 0.0)));
    assert!(close(points[3], Point::new(20.0, 10.0)));
    assert!(close(points[9], Point::new(0.0, 10.0)));
    // The first control point leaves the top along the clockwise tangent.
    assert!(points[1].x > 10.0 && close(Point::new(0.0, points[1].y), Point::new(0.0, 0.0)));
}

#[cfg(feature = "svg")]
#[test]
fn svg_path_data_becomes_absolute_segments() {
    use aegle_scene::{Path, SceneError};
    let triangle = Path::from_svg("M 2 2 L 22 2 l -10 18 z").unwrap();
    assert_eq!(
        triangle.verbs(),
        [Verb::Move, Verb::Line, Verb::Line, Verb::Close]
    );
    assert!(close(triangle.points()[2], Point::new(12.0, 20.0)));
    let arc = Path::from_svg("M0 10 A 10 10 0 0 1 20 10").unwrap();
    assert!(arc.verbs()[1..].iter().all(|verb| *verb == Verb::Cubic));
    assert!(close(*arc.points().last().unwrap(), Point::new(20.0, 10.0)));
    assert!(Path::from_svg("").unwrap().verbs().is_empty());
    assert_eq!(
        Path::from_svg("M 1").unwrap_err(),
        SceneError::InvalidPathData
    );
    assert_eq!(
        Path::from_svg("L 1 1").unwrap_err(),
        SceneError::InvalidPathData
    );
    assert_eq!(
        Path::from_svg("M 1e39 0").unwrap_err(),
        SceneError::NonFinite
    );
}
