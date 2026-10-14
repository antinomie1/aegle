//! Times editing a 2,000-line document: typing in its middle, rewrapping and
//! recoloring. Run with `cargo run -p aegle-text --release --example large_document`.

use aegle_text::{Alignment, Blob, EditorOptions, Selection, TextStyle, TextSystem};
use aegle_types::Color;
use std::{sync::Arc, time::Instant};

fn main() {
    let mut system = TextSystem::new();
    system
        .register_fonts(Blob::new(Arc::new(
            include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
        )))
        .unwrap();
    let style = TextStyle {
        families: "Aegle Test CJK",
        ..TextStyle::default()
    };
    let line = "Aegle edits retained text 你好 one line at a time 你好 BAB\n";
    let text = line.repeat(2000);
    let options = EditorOptions {
        multiline: true,
        ..Default::default()
    };
    let start = Instant::now();
    let mut editor = system.editor(&text, &style, options).unwrap();
    system
        .edit(&mut editor)
        .reflow(Some(400.0), Alignment::Left)
        .unwrap();
    println!("open and wrap: {:?}", start.elapsed());

    let middle = line.len() * 1000;
    let caret = Selection {
        anchor: middle,
        focus: middle,
    };
    system.edit(&mut editor).select(caret).unwrap();
    let start = Instant::now();
    for _ in 0..100 {
        system.edit(&mut editor).insert("a").unwrap();
    }
    println!("type one character: {:?}", start.elapsed() / 100);

    let start = Instant::now();
    for width in 0..20 {
        let width = 300.0 + width as f32 * 10.0;
        let mut driver = system.edit(&mut editor);
        driver.reflow(Some(width), Alignment::Left).unwrap();
    }
    println!("rewrap: {:?}", start.elapsed() / 20);

    let start = Instant::now();
    for shade in 0..20 {
        let recolored = TextStyle {
            color: Color::rgb(shade, 0, 0),
            ..style.clone()
        };
        system.edit(&mut editor).restyle(&recolored).unwrap();
    }
    println!("recolor: {:?}", start.elapsed() / 20);
}
