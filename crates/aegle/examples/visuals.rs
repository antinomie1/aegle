//! Shared images, a custom-painted canvas and a 10,000-row virtual list.
use std::{cell::Cell, rc::Rc};

use aegle::{
    prelude::*,
    ui::scene::{Affine, FillRule, Image, PathBuilder, Point, Stroke},
};

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Aegle — images, paths and lists")?;
    window.set_padding(16.0);
    let row = window.row();
    let pixels = (0..48 * 48)
        .flat_map(|i| [(i % 48 * 5) as u8, (i / 48 * 5) as u8, 160, 255])
        .collect();
    row.image(&Image::new(48, 48, pixels)?);
    let mut star = PathBuilder::new();
    star.move_to(Point::new(0.0, -20.0));
    for i in 1..5 {
        let angle = i as f32 * 4.0 * std::f32::consts::PI / 5.0;
        star.line_to(Point::new(20.0 * angle.sin(), -20.0 * angle.cos()));
    }
    star.close();
    let star = star.finish(FillRule::NonZero);
    let turn = Rc::new(Cell::new(0.0f32));
    let angle = turn.clone();
    let canvas = row.canvas(move |builder, size| {
        let (sin, cos) = angle.get().sin_cos();
        let center = Affine::translation(size.width / 2.0, size.height / 2.0);
        let spin = Affine::new([cos, sin, -sin, cos, 0.0, 0.0]).unwrap();
        builder.push_transform(spin.then(center).unwrap());
        builder.fill_path(&star, Color::rgb(200, 60, 90));
        builder.stroke_path(&star, Color::rgb(30, 30, 30), Stroke::new(1.5));
        builder.pop();
    });
    canvas.set_width(Some(48.0));
    canvas.set_height(Some(48.0));
    let rotate = row.button("Rotate");
    rotate.on_click(move |_| {
        turn.set(turn.get() + 0.3);
        canvas.invalidate()
    });
    window.list_view(28.0, 10_000, |row, index| {
        row.text(&format!("Row {index}"));
    });
    app.run()
}
