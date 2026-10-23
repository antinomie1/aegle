//! Image and file picker elements: files on disk, failures and native dialogs.

use aegle_loader::{Data, Elements, Program};
use aegle_ui::{Rect, Result, Visit};
use aegle_widgets::ImageView;

mod common;
use common::{names, ui};

/// A 1 × 1 opaque red PNG.
const PNG: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

fn program(source: &str) -> Result<Program> {
    Program::from_sources("main.aegle", &Elements::new(), &mut |_| Ok(source.into()))
}

/// The destination of every image drawn, in window coordinates.
fn drawn(ui: &aegle_ui::Ui) -> Result<Vec<Rect>> {
    let mut rects = Vec::new();
    ui.visit_scenes(|visit| {
        if let Visit::Scene {
            scene, transform, ..
        } = visit
        {
            for command in scene.commands() {
                if let aegle_scene::Command::Image { rect, .. } = command {
                    let origin = transform.map_point(rect.origin);
                    rects.push(Rect::new(
                        origin.x,
                        origin.y,
                        rect.size.width,
                        rect.size.height,
                    ));
                }
            }
        }
        Ok(())
    })?;
    Ok(rects)
}

#[test]
fn images_load_from_files_and_report_failures() -> Result {
    let dir = std::env::temp_dir().join(format!("aegle-media-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let skin = dir.join("skin.png");
    std::fs::write(&skin, PNG)?;
    let ui = ui()?;
    ui.root().set_padding(0.0);
    // The first load fails while the document is still being built, before
    // its `on failed` block exists; the block still hears about it.
    let view = program(
        r#"Column {
            state shown: string = "missing/skin.png"
            state error: string = ""
            Image { id: preview; source: shown; fit: contain; width: 64dp; height: 32dp
                on failed { error = self.error } }
            Text { text: error }
        }"#,
    )?
    .build(&ui.root())?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    let image = view
        .handle("preview")
        .unwrap()
        .typed::<ImageView>()
        .unwrap();
    assert!(image.image().is_none());
    assert!(
        names(&ui)?
            .iter()
            .any(|text| text.starts_with("missing/skin.png: "))
    );
    assert!(drawn(&ui)?.is_empty());

    view.set("shown", Data::String(skin.to_str().unwrap().into()))?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    assert_eq!(image.image().map(|i| (i.width(), i.height())), Some((1, 1)));
    assert_eq!(image.error(), None);
    // Contained: the square image is centered in the wide box.
    assert_eq!(drawn(&ui)?, [Rect::new(16.0, 0.0, 32.0, 32.0)]);

    // Not an image: emptied again, and reported.
    let bad = dir.join("bad.png");
    std::fs::write(&bad, b"not a png")?;
    view.set("shown", Data::String(bad.to_str().unwrap().into()))?;
    ui.dispatch_callbacks()?;
    ui.refresh()?;
    assert!(image.image().is_none() && image.error().is_some());
    view.set("shown", Data::String("".into()))?;
    assert_eq!(image.error(), None);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
