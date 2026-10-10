//! Renders every default control in its states to PNG for the developer docs.
//!
//! Headless: each state is its own retained `Ui`, drawn by the same software
//! renderer and scene path as native windows, with the bundled test font at a
//! 2x device scale. Controls use the native App's default 120 ms transitions,
//! captured after they finish. No compositor is needed.
//!
//! `cargo run -p aegle --example gallery [-- OUTPUT_DIR]`
mod composite;

use aegle::{
    Container, Modifiers, Point, PointerId, PointerKind, Result, Selection, Size, TextSystem,
    Theme, Transition, Ui, Visit, Widgets,
    scene::{Affine, Color, FillRule, Image, PathBuilder, Rect, Stroke},
};
use aegle_render_software::{Renderer, Surface};
use aegle_text::{Blob, GenericFamily};
use std::{
    cell::RefCell, fs::File, io::BufWriter, ops::Deref, path::Path, rc::Rc, sync::Arc,
    time::Duration,
};

const SCALE: f32 = 2.0;

/// Builds one cell's control into `host` and puts it into the shown state.
pub(crate) type Setup = fn(&Ui, &Container) -> Result;

fn center(node: &aegle::Node) -> Result<Point> {
    let bounds = node.bounds()?;
    Ok(Point::new(
        bounds.origin.x + bounds.size.width / 2.0,
        bounds.origin.y + bounds.size.height / 2.0,
    ))
}

pub(crate) fn hover(ui: &Ui, node: &impl Deref<Target = aegle::Node>) -> Result {
    ui.refresh()?;
    let at = center(node)?;
    ui.pointer(PointerId(1), PointerKind::Move, at, Modifiers::default())
}

fn press(ui: &Ui, node: &impl Deref<Target = aegle::Node>) -> Result {
    hover(ui, node)?;
    let at = center(node)?;
    ui.pointer(
        PointerId(1),
        PointerKind::Down { clicks: 1 },
        at,
        Modifiers::default(),
    )
}

/// Renders the cells side by side into one PNG.
fn gallery(
    fonts: &Rc<RefCell<TextSystem>>,
    theme: Theme,
    cell: Size,
    cells: &[(&str, Setup)],
    path: &Path,
) -> Result {
    let mut uis = Vec::new();
    for (caption, setup) in cells {
        let ui = Ui::with_fonts(fonts.clone(), theme)?;
        ui.set_default_transition(Some(Transition::default()))?;
        ui.resize(cell)?;
        let root = ui.root();
        root.set_padding(12.0)?;
        root.set_gap(8.0, 8.0)?;
        let label = root.text(caption)?;
        label.set_font_size(11.0)?;
        label.set_foreground(theme.muted)?;
        let host = root.column()?;
        setup(&ui, &host)?;
        ui.refresh()?;
        ui.dispatch_callbacks()?;
        ui.refresh()?;
        // Show the resting state after state transitions, as a native window does.
        ui.advance_animations(Duration::from_secs(1))?;
        ui.refresh()?;
        uis.push(ui);
    }
    let (width, height) = (
        (cell.width * cells.len() as f32 * SCALE) as u32,
        (cell.height * SCALE) as u32,
    );
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut surface = Surface::new(&mut pixels, width, height)?;
    let mut renderer = Renderer::default();
    let mut frame = renderer.begin_frame(&mut surface, theme.background);
    for (index, ui) in uis.iter().enumerate() {
        let x = index as f32 * cell.width;
        let place = Affine::translation(x, 0.0)?.then(Affine::scale(SCALE, SCALE)?)?;
        // Keep each state inside its own cell, together with any scroll clip.
        let cell_clip = |clip: Option<Rect>| {
            let clip = clip.unwrap_or(Rect::new(0.0, 0.0, cell.width, cell.height));
            let left = clip.origin.x.max(0.0);
            let right = (clip.origin.x + clip.size.width).min(cell.width);
            let top = clip.origin.y.max(0.0);
            let bottom = (clip.origin.y + clip.size.height).min(cell.height);
            Rect::new(
                (x + left) * SCALE,
                top * SCALE,
                (right - left).max(0.0) * SCALE,
                (bottom - top).max(0.0) * SCALE,
            )
        };
        ui.visit_scenes(|visit| {
            match visit {
                Visit::Scene {
                    scene,
                    transform,
                    clip,
                } => frame.draw_clipped(scene, transform.then(place)?, Some(cell_clip(clip)))?,
                Visit::PushLayer(layer) => {
                    let clip = cell_clip(layer.clip());
                    frame.push_layer(&layer.then(place)?.with_clip(Some(clip))?)?
                }
                Visit::PopLayer => frame.pop_layer()?,
            }
            Ok(())
        })?;
    }
    drop(frame);
    let mut encoder = png::Encoder::new(BufWriter::new(File::create(path)?), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(surface.data())?;
    println!("wrote {}", path.display());
    Ok(())
}

fn gradient() -> Result<Image> {
    let pixels = (0..48 * 48)
        .flat_map(|i| [(i % 48 * 5) as u8, (i / 48 * 5) as u8, 160, 255])
        .collect();
    Ok(Image::new(48, 48, pixels)?)
}

fn form(_: &Ui, host: &Container) -> Result {
    host.text("Settings")?.set_font_size(16.0)?;
    host.text_field("Hello, 世界")?;
    let row = host.row()?;
    row.button("Save")?;
    row.button("Cancel")?.set_enabled(false)?;
    host.check_box("Check box", true)?;
    host.switch("Switch", true)?;
    host.slider(0.0, 100.0, 60.0)?;
    host.progress(0.0, 100.0, 40.0)?;
    Ok(())
}

fn main() -> Result {
    let output = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/developer/images").into()
    });
    let dir = Path::new(&output);
    std::fs::create_dir_all(dir)?;
    let mut text = TextSystem::new();
    let families = text.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    text.collection_mut()
        .set_generic_families(GenericFamily::SansSerif, families.iter().map(|(id, _)| *id));
    let fonts = Rc::new(RefCell::new(text));
    let light = Theme::light();
    let shot = |name: &str, cell: (f32, f32), cells: &[(&str, Setup)]| {
        let path = dir.join(format!("{name}.png"));
        gallery(&fonts, light, Size::new(cell.0, cell.1), cells, &path)
    };

    shot(
        "text",
        (220.0, 100.0),
        &[
            ("default", |_, h| h.text("Hello, 世界").map(drop)),
            ("font_size 20", |_, h| {
                h.text("Hello, 世界")?.set_font_size(20.0)
            }),
            ("wraps to width", |_, h| {
                h.text("Text wraps to the width of its parent.").map(drop)
            }),
        ],
    )?;
    shot(
        "button",
        (140.0, 84.0),
        &[
            ("normal", |_, h| h.button("Button").map(drop)),
            ("hovered", |ui, h| hover(ui, &h.button("Button")?)),
            ("pressed", |ui, h| press(ui, &h.button("Button")?)),
            ("focused", |_, h| h.button("Button")?.focus()),
            ("disabled", |_, h| h.button("Button")?.set_enabled(false)),
        ],
    )?;
    shot(
        "text-field",
        (190.0, 84.0),
        &[
            ("empty", |_, h| h.text_field("").map(drop)),
            ("text", |_, h| h.text_field("Hello, 世界").map(drop)),
            ("focused, selection", |_, h| {
                let field = h.text_field("Hello, 世界")?;
                field.focus()?;
                field.select(Selection {
                    anchor: 0,
                    focus: 5,
                })
            }),
            ("read_only", |_, h| {
                h.text_field("Read only")?.set_read_only(true)
            }),
            ("password", |_, h| {
                h.text_field("secret")?.set_password(true)
            }),
            ("disabled", |_, h| {
                h.text_field("Disabled")?.set_enabled(false)
            }),
        ],
    )?;
    shot(
        "text-area",
        (240.0, 196.0),
        &[
            ("multiline", |_, h| {
                h.text_area("First line\nSecond line\n你好, 世界").map(drop)
            }),
            ("overflow, scroll bar", |_, h| {
                let lines: Vec<String> = (1..=12).map(|i| format!("Line {i}")).collect();
                h.text_area(&lines.join("\n"))?.set_height(Some(110.0))
            }),
            ("focused", |_, h| h.text_area("Caret after text")?.focus()),
        ],
    )?;
    shot(
        "check-box",
        (150.0, 72.0),
        &[
            ("unchecked", |_, h| h.check_box("Option", false).map(drop)),
            ("checked", |_, h| h.check_box("Option", true).map(drop)),
            ("mixed", |_, h| h.check_box("Option", true)?.set_mixed(true)),
            ("hovered", |ui, h| hover(ui, &h.check_box("Option", false)?)),
            ("focused", |_, h| h.check_box("Option", true)?.focus()),
            ("disabled", |_, h| {
                h.check_box("Option", true)?.set_enabled(false)
            }),
        ],
    )?;
    shot(
        "switch",
        (150.0, 72.0),
        &[
            ("off", |_, h| h.switch("Option", false).map(drop)),
            ("on", |_, h| h.switch("Option", true).map(drop)),
            ("hovered", |ui, h| hover(ui, &h.switch("Option", false)?)),
            ("focused", |_, h| h.switch("Option", true)?.focus()),
            ("disabled", |_, h| {
                h.switch("Option", true)?.set_enabled(false)
            }),
        ],
    )?;
    shot(
        "slider",
        (190.0, 72.0),
        &[
            ("value 30", |_, h| h.slider(0.0, 100.0, 30.0).map(drop)),
            ("step 25", |_, h| h.slider(0.0, 100.0, 50.0)?.set_step(25.0)),
            ("hovered", |ui, h| hover(ui, &h.slider(0.0, 100.0, 50.0)?)),
            ("focused", |_, h| h.slider(0.0, 100.0, 70.0)?.focus()),
            ("disabled", |_, h| {
                h.slider(0.0, 100.0, 70.0)?.set_enabled(false)
            }),
        ],
    )?;
    shot(
        "progress",
        (190.0, 64.0),
        &[
            ("0 %", |_, h| h.progress(0.0, 100.0, 0.0).map(drop)),
            ("40 %", |_, h| h.progress(0.0, 100.0, 40.0).map(drop)),
            ("100 %", |_, h| h.progress(0.0, 100.0, 100.0).map(drop)),
        ],
    )?;
    shot(
        "scroll-view",
        (220.0, 170.0),
        &[
            ("vertical overflow", |_, h| {
                let view = h.scroll_view()?;
                view.set_height(Some(110.0))?;
                for i in 1..=8 {
                    view.text(&format!("Item {i}"))?;
                }
                Ok(())
            }),
            ("scrolled", |_, h| {
                let view = h.scroll_view()?;
                view.set_height(Some(110.0))?;
                for i in 1..=8 {
                    view.text(&format!("Item {i}"))?;
                }
                view.scroll_to(Point::new(0.0, 80.0))
            }),
            ("both axes", |_, h| {
                let view = h.scroll_view()?;
                view.set_width(Some(190.0))?;
                view.set_height(Some(110.0))?;
                for i in 1..=6 {
                    view.text(&format!("Row {i}: a line wider than the viewport"))?
                        .set_width(Some(320.0))?;
                }
                Ok(())
            }),
        ],
    )?;
    shot(
        "list-view",
        (220.0, 190.0),
        &[
            ("10,000 rows", |_, h| {
                let list = h.list_view(28.0, 10_000, |row, index| {
                    row.text(&format!("Row {index}")).map(drop)
                })?;
                list.set_height(Some(140.0))
            }),
            ("scrolled to row 5,000", |ui, h| {
                let list = h.list_view(28.0, 10_000, |row, index| {
                    row.text(&format!("Row {index}")).map(drop)
                })?;
                list.set_height(Some(140.0))?;
                ui.refresh()?;
                list.scroll_to(Point::new(0.0, 5000.0 * 28.0))
            }),
        ],
    )?;
    shot(
        "image-view",
        (160.0, 120.0),
        &[
            ("pixel size", |_, h| h.image(&gradient()?).map(drop)),
            ("96x48", |_, h| {
                let image = h.image(&gradient()?)?;
                image.set_width(96.0)?;
                image.set_height(48.0)
            }),
        ],
    )?;
    shot(
        "canvas",
        (160.0, 120.0),
        &[("custom painter", |_, h| {
            let mut star = PathBuilder::new();
            star.move_to(aegle::scene::Point::new(0.0, -28.0));
            for i in 1..5 {
                let angle = i as f32 * 4.0 * std::f32::consts::PI / 5.0;
                star.line_to(aegle::scene::Point::new(
                    28.0 * angle.sin(),
                    -28.0 * angle.cos(),
                ));
            }
            star.close();
            let star = star.finish(FillRule::NonZero)?;
            let canvas = h.canvas(move |builder, size| {
                builder
                    .push_transform(Affine::translation(size.width / 2.0, size.height / 2.0)?)?;
                builder.fill_path(&star, Color::rgb(53, 92, 218))?;
                builder.stroke_path(&star, Color::rgb(32, 36, 43), Stroke::new(1.5))?;
                builder.pop()?;
                Ok(())
            })?;
            canvas.set_width(64.0)?;
            canvas.set_height(64.0)
        })],
    )?;
    shot(
        "layout",
        (230.0, 150.0),
        &[
            ("column, gap 8", |_, h| {
                let column = h.column()?;
                column.set_padding(8.0)?;
                column.set_border_width(1.0)?;
                column.set_border_color(Theme::light().border)?;
                column.button("One")?;
                column.button("Two")?;
                Ok(())
            }),
            ("row, gap 8", |_, h| {
                let row = h.row()?;
                row.set_padding(8.0)?;
                row.set_border_width(1.0)?;
                row.set_border_color(Theme::light().border)?;
                row.button("One")?;
                row.button("Two")?;
                Ok(())
            }),
            ("grow 1", |_, h| {
                let row = h.row()?;
                row.button("Fixed")?;
                row.button("Grows")?.set_grow(1.0)?;
                Ok(())
            }),
        ],
    )?;
    shot(
        "styles",
        (150.0, 84.0),
        &[
            ("default", |_, h| h.button("Button").map(drop)),
            ("radius 8", |_, h| h.button("Button")?.set_radius(8.0)),
            ("colors", |_, h| {
                let button = h.button("Button")?;
                button.set_background(Color::rgb(103, 80, 164))?;
                button.set_foreground(Color::WHITE)?;
                button.set_border_width(0.0)
            }),
            ("local dark theme", |_, h| {
                h.set_theme(Some(Theme::dark()))?;
                h.set_padding(8.0)?;
                h.set_background(Theme::dark().background)?;
                h.button("Button").map(drop)
            }),
        ],
    )?;
    composite::shots(&shot)?;
    for (name, theme) in [
        ("theme-light", Theme::light()),
        ("theme-dark", Theme::dark()),
        ("theme-high-contrast", Theme::high_contrast()),
    ] {
        let path = dir.join(format!("{name}.png"));
        gallery(
            &fonts,
            theme,
            Size::new(280.0, 360.0),
            &[(name, form)],
            &path,
        )?;
    }
    Ok(())
}
