//! The default controls without the `aegle` facade or a native window: the
//! `aegle-ui` engine, `aegle-widgets` controls, an `aegle-theme` theme and
//! `aegle-motion` timing, driven by a minimal host that feeds input, runs
//! callbacks, samples animations and draws with the software renderer into
//! `target/aegle-standalone.ppm`.
//!
//! `cargo run -p aegle-widgets --features motion --example standalone`

use aegle_ui::OrFail;
use std::{cell::RefCell, fs, rc::Rc, sync::Arc, time::Duration};

use aegle_motion::{Spring, Transition};
use aegle_render_software::{Renderer, Surface};
use aegle_text::{Blob, GenericFamily, TextSystem};
use aegle_theme::Theme;
use aegle_ui::{Modifiers, Point, PointerId, PointerKind, Result, Size, Ui, Visit};
use aegle_widgets::Widgets;

fn main() -> Result {
    // Fonts are explicit; `aegle-ui/system-fonts` would add system discovery.
    let mut text = TextSystem::new();
    let font = include_bytes!("../../../tests/assets/aegle-test-cjk.otf");
    let families = text.register_fonts(Blob::new(Arc::new(font.as_slice())))?;
    let ids = families.iter().map(|(id, _)| *id);
    text.collection_mut()
        .set_generic_families(GenericFamily::SansSerif, ids);
    let theme = Theme {
        radius: 6.0,
        ..Theme::light()
    };
    let ui = Ui::with_fonts(Rc::new(RefCell::new(text)), theme)?;
    // A native App installs this default; an embedding chooses for itself.
    ui.set_default_transition(Some(Transition::default()));

    let root = ui.root();
    let status = root.text("Not saved");
    let row = root.row();
    let save = row.button("Save");
    let undo = row.button("Undo");
    undo.set_enabled(false);
    let label = status.clone();
    save.on_click(move |_| {
        label.set_text("Saved, 你好");
        let spring = Transition::spring(Spring::new(300.0, 14.0).or_fail());
        label.with_transition(spring, || label.set_offset(Point::new(16.0, 0.0)))
    });

    // The host's side: size, input, callbacks, the animation clock, refresh
    // and drawing. A real host forwards its window events the same way.
    ui.resize(Size::new(240.0, 100.0));
    ui.refresh()?;
    let bounds = save.bounds();
    let at = Point::new(bounds.origin.x + 8.0, bounds.origin.y + 8.0);
    let (pointer, none) = (PointerId(1), Modifiers::default());
    ui.pointer(pointer, PointerKind::Down { clicks: 1 }, at, none)?;
    ui.pointer(pointer, PointerKind::Up, at, none)?;
    ui.dispatch_callbacks()?;
    let mut now = Duration::ZERO;
    ui.advance_animations(now)?;
    ui.refresh()?;
    while ui.has_animations() {
        now += Duration::from_millis(16);
        ui.advance_animations(now)?;
        ui.refresh()?;
    }
    println!(
        "{:?} at x = {} after {} ms",
        status.text(),
        status.bounds().origin.x,
        now.as_millis()
    );
    draw(&ui, theme, "target/aegle-standalone.ppm")
}

/// Draws every retained scene into an RGBA buffer and writes it as a PPM.
fn draw(ui: &Ui, theme: Theme, path: &str) -> Result {
    let (width, height) = (240, 100);
    let mut pixels = vec![0; width * height * 4];
    let mut surface = Surface::new(&mut pixels, width as u32, height as u32)?;
    let mut renderer = Renderer::default();
    {
        let mut frame = renderer.begin_frame(&mut surface, theme.background);
        // Records, and the layers of translucent or blurring subtrees.
        ui.visit_scenes(|visit| {
            match visit {
                Visit::Scene {
                    scene,
                    transform,
                    clip,
                } => frame.draw_clipped(scene, transform, clip)?,
                Visit::PushLayer(layer) => frame.push_layer(&layer)?,
                Visit::PopLayer => frame.pop_layer()?,
            }
            Ok(())
        })?;
    }
    let mut ppm = format!("P6 {width} {height} 255\n").into_bytes();
    ppm.extend(surface.data().chunks(4).flat_map(|p| [p[0], p[1], p[2]]));
    fs::create_dir_all("target")?;
    fs::write(path, ppm)?;
    println!("wrote {path}");
    Ok(())
}
