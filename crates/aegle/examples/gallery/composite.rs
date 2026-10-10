//! Choice, popup, table, content-sized list and group effect screenshots.
use crate::{Setup, hover};
use aegle::{
    Color, Container, Key, KeyInput, Modifiers, NodeMenu, NodePopup, Point, Result, TableColumn,
    Ui, Widgets,
};
use aegle_ui::OrFail;

/// Two overlapping squares, red then blue, at `alpha`.
fn squares(h: &Container, alpha: u8) -> Result<Container> {
    let pair = h.row();
    pair.set_gap(0.0, 0.0);
    for (color, offset) in [((220, 40, 40), 0.0), ((40, 80, 220), -24.0)] {
        let square = pair.column();
        square.set_width(48.0);
        square.set_height(48.0);
        square.set_radius(6.0);
        square.set_offset(Point::new(offset, 12.0 + offset / 2.0));
        square.set_background(Color::rgba(color.0, color.1, color.2, alpha));
    }
    Ok(pair)
}

/// Stripes under a frosted panel that blurs them.
fn frosted(h: &Container) -> Result {
    let stripes = h.row();
    stripes.set_gap(6.0, 6.0);
    for _ in 0..8 {
        let stripe = stripes.column();
        stripe.set_width(8.0);
        stripe.set_height(72.0);
        stripe.set_background(Color::rgb(40, 80, 220));
    }
    let panel = h.column();
    panel.set_width(80.0);
    panel.set_height(40.0);
    panel.set_radius(8.0);
    panel.set_offset(Point::new(20.0, -64.0));
    panel.set_background(Color::rgba(255, 255, 255, 90));
    panel.set_backdrop_blur(4.0);
    Ok(())
}

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
    h.radio("Small", false);
    h.radio("Medium", true);
    h.radio("Large", false);
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
        cell.text(&text);
    });
    table.set_height(Some(150.0));
    Ok(())
}

type Shot<'a> = &'a dyn Fn(&str, (f32, f32), &[(&str, Setup)]) -> Result;

pub(crate) fn shots(shot: Shot) -> Result {
    shot(
        "layers",
        (150.0, 130.0),
        &[
            ("translucent colors", |_, h| {
                squares(h, 128).map(drop).or_fail()
            }),
            ("group opacity 0.5", |_, h| {
                squares(h, 255).or_fail().set_opacity(0.5);
            }),
            ("backdrop blur", |_, h| frosted(h).or_fail()),
        ],
    )?;
    shot(
        "radio",
        (150.0, 176.0),
        &[
            ("group", |_, h| group(h).or_fail()),
            ("hovered", |ui, h| {
                hover(ui, &h.radio("Option", false)).or_fail()
            }),
            ("focused", |_, h| h.radio("Option", true).focus()),
            ("disabled", |_, h| {
                h.radio("Option", true).set_enabled(false);
            }),
        ],
    )?;
    shot(
        "dropdown",
        (200.0, 200.0),
        &[
            ("closed", |_, h| {
                h.dropdown(&["Red", "Green", "Blue"], 1);
            }),
            ("open", |ui, h| {
                h.dropdown(&["Red", "Green", "Blue"], 1).focus();
                ui.refresh().or_fail();
                enter(ui).or_fail()
            }),
            ("focused", |_, h| {
                h.dropdown(&["Red", "Green", "Blue"], 1).focus();
            }),
        ],
    )?;
    shot(
        "popup",
        (220.0, 180.0),
        &[
            ("hidden", |_, h| {
                h.button("Menu");
            }),
            ("shown below its anchor", |ui, h| {
                let popup = h.button("Menu").popup();
                popup.text("Popup content");
                popup.button("Action");
                ui.refresh().or_fail();
                popup.show();
            }),
        ],
    )?;
    shot(
        "menu",
        (300.0, 200.0),
        &[
            ("menu bar with a submenu", |ui, h| {
                let bar = h.menu_bar();
                let file = bar.menu("File");
                bar.menu("Edit");
                file.item("New");
                let recent = file.submenu("Open recent");
                recent.item("notes.txt");
                file.separator();
                file.check_item("Autosave", true);
                ui.refresh().or_fail();
                file.show();
                ui.refresh().or_fail();
                recent.show();
            }),
            ("context menu at a point", |ui, h| {
                let menu = h.context_menu();
                menu.item("Cut");
                menu.item("Copy").set_enabled(false);
                menu.item("Paste");
                ui.refresh().or_fail();
                menu.show_at(Point::new(40.0, 30.0));
            }),
        ],
    )?;
    shot(
        "table",
        (380.0, 200.0),
        &[("3 columns, 100 rows", |_, h| table(h).or_fail())],
    )?;
    shot(
        "variable-list",
        (260.0, 230.0),
        &[("rows sized to content", |_, h| {
            let list = h.variable_list_view(24.0, 50, |row, index| {
                row.set_padding(4.0);
                let words = ["Short row.", "A longer row that wraps onto two lines here."];
                row.text(&format!("{index}. {}", words[index % 2]));
            });
            list.set_height(Some(170.0));
        })],
    )
}
