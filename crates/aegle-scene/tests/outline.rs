//! Outlines from SVG path data.
#![cfg(feature = "svg")]

use aegle_scene::{Path, Point, SceneError, Verb};

fn close(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

#[test]
fn svg_path_data_becomes_absolute_segments() {
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
