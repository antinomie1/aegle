//! Requests one desktop service and prints the events that follow:
//! `cargo run -p aegle-desktop --example desktop -- open|save|notify|tray|shortcut`.
use std::{sync::mpsc, time::Duration};

use aegle_desktop::{Desktop, FileDialog, Icon, MenuItem, Notification, Shortcut, Tray};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tx, events) = mpsc::channel();
    let desktop = Desktop::new("org.aegle.DesktopExample", move |event| {
        let _ = tx.send(event);
    })?;
    let rgba: Vec<u8> = (0..32 * 32)
        .flat_map(|i| [40, 80 + (i % 32 * 5) as u8, 220, 255])
        .collect();
    let menu = [
        MenuItem::Item {
            id: 1,
            label: "Hello",
            enabled: true,
            checked: None,
        },
        MenuItem::Separator,
        MenuItem::Item {
            id: 2,
            label: "Quit",
            enabled: true,
            checked: None,
        },
    ];
    match std::env::args().nth(1).as_deref().unwrap_or("open") {
        "open" => drop(desktop.open_file(&FileDialog {
            title: "Open a text file",
            filters: &[("Text", &["*.txt"]), ("All files", &["*"])],
            ..Default::default()
        })?),
        "save" => drop(desktop.save_file(&FileDialog {
            title: "Save",
            name: "notes.txt",
            ..Default::default()
        })?),
        "notify" => drop(desktop.notify(&Notification {
            summary: "Hello from Aegle",
            body: "A notification with one action.",
            actions: &[("open", "Open")],
        })?),
        "tray" => desktop.set_tray(Some(&Tray {
            icon: Icon {
                width: 32,
                height: 32,
                rgba: &rgba,
            },
            tooltip: "Aegle example",
            menu: &menu,
        }))?,
        _ => desktop.bind_shortcuts(&[Shortcut {
            id: "hello",
            description: "Say hello",
            trigger: "Ctrl+Alt+H".parse()?,
        }])?,
    }
    while let Ok(event) = events.recv_timeout(Duration::from_secs(60)) {
        println!("{event:?}");
        if matches!(event, aegle_desktop::Event::Files { .. }) {
            break;
        }
    }
    Ok(())
}
