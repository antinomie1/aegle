//! A file dialog, a notification and a tray icon with a menu; the window
//! shows what the desktop reports.
use std::rc::Rc;

use aegle::{
    desktop::{Event, FileDialog, Icon, MenuItem, Notification, Tray},
    prelude::*,
};

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Desktop services")?;
    window.set_padding(16.0);
    let status = window.text("Nothing yet");
    let shown = status.clone();
    let desktop = Rc::new(app.desktop("org.aegle.DesktopDemo", move |event| {
        let text = match event {
            Event::Files {
                paths: Some(paths), ..
            } => format!("Chose {paths:?}"),
            Event::Files { paths: None, .. } => "Cancelled".into(),
            Event::TrayMenu { item: 2 } => return shown.set_text("Quit chosen from the tray"),
            event => format!("{event:?}"),
        };
        shown.set_text(&text)
    })?);
    let rgba: Vec<u8> = (0..32 * 32).flat_map(|_| [53, 92, 218, 255]).collect();
    desktop.set_tray(Some(&Tray {
        icon: Icon {
            width: 32,
            height: 32,
            rgba: &rgba,
        },
        tooltip: "Aegle desktop demo",
        menu: &[
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
        ],
    }))?;
    let row = window.row();
    let opener = desktop.clone();
    row.button("Open…").on_click(move |_| {
        let filters: &[(&str, &[&str])] = &[("Text", &["*.txt", "*.md"])];
        opener.open_file(&FileDialog {
            title: "Open",
            filters,
            ..Default::default()
        })?;
        Ok(())
    });
    row.button("Notify").on_click(move |_| {
        desktop.notify(&Notification {
            summary: "Hello",
            body: "From Aegle",
            actions: &[],
        })?;
        Ok(())
    });
    app.run()
}
