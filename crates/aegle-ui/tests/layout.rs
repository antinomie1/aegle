//! Container and item layout settings resolve through Taffy, survive theme
//! changes, and reject invalid values at the boundary.
use std::{cell::RefCell, rc::Rc};

use aegle_ui::{
    Align, Container, Direction, Insets, Justify, Length, Node, Result, Size, TextSystem, Theme,
    Ui, Wrap,
};

fn ui() -> Result<Ui> {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.root().set_padding(0.0)?;
    ui.root().set_gap(0.0)?;
    ui.resize(Size::new(400.0, 300.0))?;
    Ok(ui)
}

fn boxed(parent: &Container, width: f32, height: f32) -> Result<Container> {
    let node = parent.column()?;
    node.set_size(width, height)?;
    Ok(node)
}

fn at(node: &Node) -> Result<(f32, f32, f32, f32)> {
    let b = node.bounds()?;
    Ok((b.origin.x, b.origin.y, b.size.width, b.size.height))
}

#[test]
fn flex_alignment_sizing_wrap_and_absolute_placement() -> Result {
    let ui = ui()?;
    let root = ui.root();
    let row = root.row()?;
    row.set_size(Length::Percent(100.0), 60.0)?;
    row.set_justify_content(Some(Justify::SpaceBetween))?;
    row.set_align_items(Some(Align::Center))?;
    let (a, b) = (boxed(&row, 50.0, 20.0)?, boxed(&row, 50.0, 40.0)?);
    b.set_align_self(Some(Align::End))?;

    let centered = root.column()?;
    centered.set_size(Length::Percent(50.0), 10.0)?;
    centered.set_margin(Insets::symmetric(Length::Auto, 5.0))?;

    let split = root.row()?;
    split.set_gap(0.0)?;
    let (one, two) = (split.column()?, split.column()?);
    for (part, grow) in [(&one, 1.0), (&two, 3.0)] {
        part.set_basis(0.0)?;
        part.set_grow(grow)?;
        part.set_height(10.0)?;
    }
    let clamped = boxed(&split, 10.0, 10.0)?;
    clamped.set_grow(1.0)?;
    clamped.set_max_width(30.0)?;

    let flow = root.row()?;
    flow.set_width(200.0)?;
    flow.set_gaps(0.0, 4.0)?;
    flow.set_wrap(Wrap::Wrap)?;
    let items: Vec<_> = (0..5)
        .map(|_| boxed(&flow, 60.0, 10.0))
        .collect::<Result<_>>()?;

    let square = root.column()?;
    square.set_width(30.0)?;
    square.set_aspect_ratio(Some(2.0))?;
    square.set_align_self(Some(Align::Start))?;

    let overlay = boxed(&root, 0.0, 0.0)?;
    overlay.set_size(Length::Auto, 20.0)?;
    overlay.set_absolute(Some(Insets::new(Length::Auto, 10.0, 5.0, 10.0)))?;
    // A contents group's children share the row's space distribution.
    let spread = root.row()?;
    spread.set_justify_content(Some(Justify::SpaceBetween))?;
    let first = boxed(&spread, 10.0, 4.0)?;
    let group = spread.contents()?;
    let inner = boxed(&group, 10.0, 4.0)?;
    let nested = boxed(&group.contents()?, 10.0, 4.0)?;
    let end = boxed(&spread, 10.0, 4.0)?;
    let reversed = root.row()?;
    reversed.set_direction(Direction::RowReverse)?;
    let last = boxed(&reversed, 20.0, 5.0)?;
    ui.refresh()?;

    assert_eq!(at(&a)?, (0.0, 20.0, 50.0, 20.0));
    assert_eq!(at(&b)?, (350.0, 20.0, 50.0, 40.0));
    assert_eq!(at(&centered)?, (100.0, 65.0, 200.0, 10.0));
    // 400 px minus the clamped item's 30 px, shared 1:3 from a zero basis.
    assert_eq!(
        (at(&one)?.2, at(&two)?.2, at(&clamped)?.2),
        (92.5, 277.5, 30.0)
    );
    let rows: Vec<_> = items
        .iter()
        .map(|i| at(i).map(|r| r.1))
        .collect::<Result<_>>()?;
    assert_eq!(rows[..4], [rows[0], rows[0], rows[0], rows[0] + 14.0]);
    assert_eq!(at(&square)?.3, 15.0);
    assert_eq!(at(&overlay)?, (10.0, 275.0, 380.0, 20.0));
    assert_eq!(at(&last)?.0, 380.0);
    let xs = [&first, &inner, &nested, &end].map(|n| at(n).map(|r| r.0));
    assert_eq!(xs.map(|x| x.unwrap()), [0.0, 130.0, 260.0, 390.0]);
    group.set_visible(false)?;
    ui.refresh()?;
    assert_eq!((at(&first)?.0, at(&end)?.0), (0.0, 390.0));
    assert_eq!(at(&group)?.2, 0.0);
    group.set_visible(true)?;

    // Local layout survives a theme change; theme defaults stay elsewhere.
    ui.set_theme(Theme {
        gap: 9.0,
        padding: 7.0,
        ..Theme::dark()
    })?;
    ui.refresh()?;
    assert_eq!(at(&a)?.0, 0.0);
    assert_eq!(rows[3] - rows[0], at(&items[3])?.1 - at(&items[0])?.1);

    for invalid in [
        flow.set_width(f32::NAN),
        flow.set_width(-1.0),
        flow.set_padding(Length::Auto),
        flow.set_gap(Length::Auto),
        flow.set_aspect_ratio(Some(0.0)),
        flow.set_shrink(-1.0),
        flow.set_margin(f32::INFINITY),
        a.set_max_height(Length::Percent(f32::NAN)),
    ] {
        assert!(invalid.is_err());
    }
    flow.set_margin(-4.0)?;
    root.set_padding(Insets::new(1.0, 2.0, 3.0, 4.0))?;
    ui.refresh()?;
    assert_eq!(at(&a)?.0, 4.0);
    Ok(())
}

#[cfg(feature = "grid")]
#[test]
fn grids_place_span_and_stack_children() -> Result {
    use aegle_ui::{Placement, Track};
    let ui = ui()?;
    let grid = ui
        .root()
        .grid(&[Track::Px(100.0), Track::Fr(1.0), Track::Fr(1.0)])?;
    grid.set_gap(10.0)?;
    grid.set_auto_rows(&[Track::Px(20.0)])?;
    let cells: Vec<_> = (0..4).map(|_| grid.column()).collect::<Result<_>>()?;
    let wide = grid.column()?;
    wide.set_grid_column(Placement::at(2).spanning(2))?;
    wide.set_grid_row(Placement::at(3))?;
    let narrow = grid.column()?;
    narrow.set_size(20.0, 10.0)?;
    narrow.set_justify_self(Some(aegle_ui::Align::End))?;
    narrow.set_align_self(Some(aegle_ui::Align::Center))?;

    let generated = grid.contents()?;
    let tail: Vec<_> = (0..2).map(|_| generated.column()).collect::<Result<_>>()?;
    let stack = ui.root().stack()?;
    stack.set_height(50.0)?;
    let back = stack.column()?;
    let badge = boxed(&stack, 10.0, 10.0)?;
    badge.set_justify_self(Some(aegle_ui::Align::End))?;
    badge.set_align_self(Some(aegle_ui::Align::Start))?;
    let layered = boxed(&stack.contents()?, 8.0, 8.0)?;
    layered.set_align_self(Some(aegle_ui::Align::Center))?;
    layered.set_justify_self(Some(aegle_ui::Align::Center))?;
    let moved = boxed(&ui.root(), 6.0, 6.0)?;
    moved.reparent(&stack)?;
    moved.set_align_self(Some(aegle_ui::Align::End))?;
    moved.set_justify_self(Some(aegle_ui::Align::Start))?;
    ui.refresh()?;

    // 400 = 100 + 2 × 10 gap + 2 × 140.
    let columns: Vec<_> = cells.iter().map(|c| at(c)).collect::<Result<_>>()?;
    assert_eq!(
        columns[..3].iter().map(|c| c.0).collect::<Vec<_>>(),
        [0.0, 110.0, 260.0]
    );
    assert_eq!(
        (columns[3].0, columns[3].1, columns[3].3),
        (0.0, 30.0, 20.0)
    );
    assert_eq!(at(&wide)?, (110.0, 60.0, 290.0, 20.0));
    // Auto-placed after the fourth cell: row 2, column 2 (110..250), end-aligned.
    assert_eq!(at(&narrow)?, (230.0, 35.0, 20.0, 10.0));
    // Generated children fill the free cells after the narrow item: row 2,
    // column 3, then row 3, column 1 beside the wide item.
    assert_eq!((at(&tail[0])?.0, at(&tail[0])?.1), (260.0, 30.0));
    assert_eq!((at(&tail[1])?.0, at(&tail[1])?.1), (0.0, 60.0));
    let top = at(&stack)?.1;
    assert_eq!(at(&back)?, (0.0, top, 400.0, 50.0));
    assert_eq!(at(&badge)?, (390.0, top, 10.0, 10.0));
    assert_eq!(at(&moved)?, (0.0, top + 44.0, 6.0, 6.0));
    assert_eq!(at(&layered)?, (196.0, top + 21.0, 8.0, 8.0));

    assert!(grid.set_columns(&[Track::Fr(-1.0)]).is_err());
    assert!(wide.set_grid_row(Placement::at(0)).is_err());
    assert!(wide.set_grid_row(Placement::span(0)).is_err());
    grid.set_columns(&[Track::MinMax(50.0, 1.0), Track::MaxContent])?;
    grid.set_flow(aegle_ui::Flow::Column)?;
    Ok(())
}
