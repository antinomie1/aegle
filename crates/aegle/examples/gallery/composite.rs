//! Choice, popup, table and content-sized list screenshots.
use crate::{Setup, hover};
use aegle::{
    Container, Key, KeyInput, Modifiers, NodeMenu, NodePopup, Point, Result, TableColumn, Ui,
    Widgets,
};

fn enter(ui: &Ui) -> Result {
    for pressed in [true, false] {
        let modifiers = Modifiers::default();
        ui.key(KeyInput {
            key: Key::Enter,
            text: "",
            modifiers,
            pressed,
            repeat: false,
        })?;
    }
    ui.dispatch_callbacks()
}

fn group(h: &Container) -> Result {
    h.radio("Small", false)?;
    h.radio("Medium", true)?;
    h.radio("Large", false)?;
    Ok(())
}

fn table(h: &Container) -> Result {
    let columns = [
        TableColumn {
            title: "Name",
            width: Some(120.0),
        },
        TableColumn {
            title: "Size",
            width: Some(70.0),
        },
        TableColumn {
            title: "Kind",
            width: None,
        },
    ];
    let table = h.table(&columns, 28.0, 100, |cell, row, column| {
        let text = match column {
            0 => format!("file-{row}.txt"),
            1 => format!("{} KB", row * 3 + 1),
            _ => "Text".into(),
        };
        cell.text(&text).map(drop)
    })?;
    table.set_height(Some(150.0))
}

type Shot<'a> = &'a dyn Fn(&str, (f32, f32), &[(&str, Setup)]) -> Result;

pub(crate) fn shots(shot: Shot) -> Result {
    shot(
        "radio",
        (150.0, 176.0),
        &[
            ("group", |_, h| group(h)),
            ("hovered", |ui, h| hover(ui, &h.radio("Option", false)?)),
            ("focused", |_, h| h.radio("Option", true)?.focus()),
            ("disabled", |_, h| {
                h.radio("Option", true)?.set_enabled(false)
            }),
        ],
    )?;
    shot(
        "dropdown",
        (200.0, 200.0),
        &[
            ("closed", |_, h| {
                h.dropdown(&["Red", "Green", "Blue"], 1).map(drop)
            }),
            ("open", |ui, h| {
                h.dropdown(&["Red", "Green", "Blue"], 1)?.focus()?;
                ui.refresh()?;
                enter(ui)
            }),
            ("focused", |_, h| {
                h.dropdown(&["Red", "Green", "Blue"], 1)?.focus()
            }),
        ],
    )?;
    shot(
        "popup",
        (220.0, 180.0),
        &[
            ("hidden", |_, h| h.button("Menu").map(drop)),
            ("shown below its anchor", |ui, h| {
                let popup = h.button("Menu")?.popup()?;
                popup.text("Popup content")?;
                popup.button("Action")?;
                ui.refresh()?;
                popup.show()
            }),
        ],
    )?;
    shot(
        "menu",
        (300.0, 200.0),
        &[
            ("menu bar with a submenu", |ui, h| {
                let bar = h.menu_bar()?;
                let file = bar.menu("File")?;
                bar.menu("Edit")?;
                file.item("New")?;
                let recent = file.submenu("Open recent")?;
                recent.item("notes.txt")?;
                file.separator()?;
                file.check_item("Autosave", true)?;
                ui.refresh()?;
                file.show()?;
                ui.refresh()?;
                recent.show()
            }),
            ("context menu at a point", |ui, h| {
                let menu = h.context_menu()?;
                menu.item("Cut")?;
                menu.item("Copy")?.set_enabled(false)?;
                menu.item("Paste")?;
                ui.refresh()?;
                menu.show_at(Point::new(40.0, 30.0))
            }),
        ],
    )?;
    shot(
        "table",
        (380.0, 200.0),
        &[("3 columns, 100 rows", |_, h| table(h))],
    )?;
    shot(
        "variable-list",
        (260.0, 230.0),
        &[("rows sized to content", |_, h| {
            let list = h.variable_list_view(24.0, 50, |row, index| {
                row.set_padding(4.0)?;
                let words = ["Short row.", "A longer row that wraps onto two lines here."];
                row.text(&format!("{index}. {}", words[index % 2]))
                    .map(drop)
            })?;
            list.set_height(Some(170.0))
        })],
    )
}
