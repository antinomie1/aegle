//! Walking a scene fills rows, returns atlas commands and enforces limits.
use aegle_gpu::{Error, Recording, Shelf, Step, Walker, viewport};
use aegle_scene::{Affine, Color, FillRule, PathBuilder, Point, Rect, RoundedRect, SceneBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Result<RoundedRect> {
    Ok(RoundedRect::new(Rect::new(x, y, w, h), 0.0)?)
}

#[test]
fn walker_records_clips_limits_and_hands_back_commands() -> Result {
    let mut builder = SceneBuilder::new();
    builder.fill(rect(0.0, 0.0, 10.0, 10.0)?, Color::BLACK)?;
    builder.push_clip(rect(2.0, 2.0, 4.0, 4.0)?)?;
    builder.fill(rect(0.0, 0.0, 10.0, 10.0)?, Color::WHITE)?;
    builder.pop()?;
    builder.fill(rect(0.0, 0.0, 10.0, 10.0)?, Color::TRANSPARENT)?;
    let mut path = PathBuilder::new();
    path.move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(4.0, 0.0))
        .line_to(Point::new(0.0, 4.0))
        .close();
    builder.fill_path(&path.finish(FillRule::NonZero)?, Color::BLACK)?;
    let scene = builder.finish()?;

    // Y flips only for WebGPU clip space, via the viewport height's sign.
    assert_eq!(viewport(8, 6, false), [8.0, 6.0]);
    assert_eq!(viewport(8, 6, true), [8.0, -6.0]);

    let mut recording = Recording::default();
    let mut walker = Walker::new(
        &scene,
        Affine::IDENTITY,
        None,
        [16, 16],
        viewport(16, 16, false),
        &mut recording,
    )?;
    let mut commands = 0;
    loop {
        match walker.step(&mut recording)? {
            Step::Done => break,
            Step::Recorded => {}
            Step::Command(..) => commands += 1,
        }
    }
    // Two visible fills; the transparent one adds no row. The path is the backend's.
    assert_eq!(
        (recording.primitives.len(), recording.clips.len(), commands),
        (2, 1, 1)
    );
    assert_eq!(
        recording.primitives[1].header[0], 0,
        "the clipped fill refers to clip 0"
    );
    assert_eq!(
        recording.primitives[1].bounds,
        [2.0, 2.0, 6.0, 6.0],
        "bounds shrink to the clip"
    );
    assert_eq!(
        recording.primitives[0].header[0],
        u32::MAX,
        "unclipped fills have no clip"
    );

    // Eight scene clips plus the external clip exceed the eight-layer limit.
    let mut builder = SceneBuilder::new();
    for _ in 0..8 {
        builder.push_clip(rect(0.0, 0.0, 8.0, 8.0)?)?;
    }
    for _ in 0..8 {
        builder.pop()?;
    }
    let deep = builder.finish()?;
    let outer = Some(Rect::new(0.0, 0.0, 8.0, 8.0));
    let result = Walker::new(
        &deep,
        Affine::IDENTITY,
        outer,
        [8, 8],
        [8.0, 8.0],
        &mut Recording::default(),
    );
    assert!(matches!(result, Err(Error::ClipDepth)));
    Ok(())
}

#[test]
fn shelf_wraps_rows_and_rejects_oversize_blocks() {
    let page = [20, 20];
    let (shelf, a) = Shelf::default().place(12, 6, page).unwrap();
    assert_eq!(a, [0, 0]);
    let (shelf, b) = shelf.place(12, 8, page).unwrap();
    assert_eq!(b, [0, 6], "a block that overflows the row starts a new one");
    let (_, c) = shelf.place(8, 4, page).unwrap();
    assert_eq!(c, [12, 6]);
    assert!(Shelf::default().place(21, 2, page).is_none());
    assert!(
        shelf.place(2, 15, page).is_none(),
        "taller than the space left"
    );
}
