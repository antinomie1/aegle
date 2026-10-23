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

#[test]
fn file_pickers_ask_the_host_and_report_the_chosen_path() -> Result {
    let dir = std::env::temp_dir().join(format!("aegle-picker-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let skin = dir.join("skin.png");
    std::fs::write(&skin, PNG)?;
    let ui = ui()?;
    let view = program(
        r#"Column {
            state skin: string = ""
            state changes: int = 0
            FilePicker { id: picker; title: "选择皮肤"; filters: ["皮肤|*.png", "*.jpg *.jpeg"]
                button_text: "浏览"; on changed { skin = self.path; changes += 1 } }
            Image { id: preview; source: skin; width: 64dp; height: 64dp }
        }"#,
    )?
    .build(&ui.root())?;
    ui.refresh()?;
    common::click(&ui, "浏览")?;
    let dialogs = ui.take_file_dialogs();
    let [(request, dialog)] = dialogs.as_slice() else {
        panic!("one dialog, not {dialogs:?}")
    };
    assert_eq!(dialog.title, "选择皮肤");
    assert_eq!(dialog.mode, aegle_ui::FileMode::Open);
    let filters = [
        ("皮肤", vec!["*.png"]),
        ("*.jpg *.jpeg", vec!["*.jpg", "*.jpeg"]),
    ];
    let filters = filters.map(|(name, patterns)| {
        (
            name.into(),
            patterns.into_iter().map(String::from).collect(),
        )
    });
    assert_eq!(dialog.filters, filters);
    ui.files_chosen(*request, Some(vec![skin.clone()]))?;
    // Answered once: a repeated answer is ignored.
    ui.files_chosen(*request, Some(vec![dir.clone()]))?;
    ui.dispatch_callbacks()?;
    assert_eq!(
        view.get("skin"),
        Some(Data::String(skin.to_str().unwrap().into()))
    );
    let image = view
        .handle("preview")
        .unwrap()
        .typed::<ImageView>()
        .unwrap();
    assert!(image.image().is_some());

    // A cancelled dialog changes nothing; the next one starts in the folder.
    common::click(&ui, "浏览")?;
    let [(request, dialog)] = ui.take_file_dialogs().try_into().unwrap();
    assert_eq!(dialog.folder.as_deref(), Some(dir.as_path()));
    ui.files_chosen(request, None)?;
    ui.dispatch_callbacks()?;
    assert_eq!(view.get("changes"), Some(Data::Int(1)));

    // A file dropped on the picker is chosen too.
    let picker = view.handle("picker").unwrap().node().bounds();
    let inside = aegle_ui::Point::new(picker.origin.x + 4.0, picker.origin.y + 4.0);
    ui.drop_data(
        inside,
        aegle_ui::DragData::Files(vec![dir.join("dropped.png")]),
    );
    ui.dispatch_callbacks()?;
    ui.dispatch_callbacks()?;
    assert_eq!(view.get("changes"), Some(Data::Int(2)));
    assert!(matches!(view.get("skin"), Some(Data::String(s)) if s.ends_with("dropped.png")));
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
