//! Every default control in one window, wired to a shared status line.
use std::{cell::Cell, rc::Rc};

use aegle::{
    prelude::*,
    scene::{Affine, FillRule, Gradient, GradientStop, Image, PathBuilder, Point, Stroke},
};

const THEMES: [&str; 3] = ["Light", "Dark", "High contrast"];

fn star() -> Result<aegle::scene::Path> {
    let mut star = PathBuilder::new();
    star.move_to(Point::new(0.0, -22.0));
    for i in 1..5 {
        let angle = i as f32 * 4.0 * std::f32::consts::PI / 5.0;
        star.line_to(Point::new(22.0 * angle.sin(), -22.0 * angle.cos()));
    }
    star.close();
    Ok(star.finish(FillRule::NonZero)?)
}

/// Editors, buttons with a popup, binary and numeric choices, and the theme dropdown.
fn inputs(window: &Window, column: &Container, status: &Label) -> Result {
    column.text("Aegle showcase")?.set_font_size(22.0)?;
    let field = column.text_field("你好，世界 / 日本語 / 한글")?;
    field.set_accessible_label("Name")?;
    let shown = status.clone();
    field.on_submit(move |field| shown.set_text(&format!("Submitted: {}", field.text()?)))?;
    column.text_field("secret")?.set_password(true)?;
    column.text_area("Multiline editor\nwith IME, selection and undo.\n多行编辑")?;

    let buttons = column.row()?;
    let shown = status.clone();
    let button = buttons.button("Button")?;
    button.set_tooltip(Some("Shows a message below"))?;
    button.on_click(move |_| shown.set_text("Button clicked"))?;
    buttons.button("Disabled")?.set_enabled(false)?;
    let menu = buttons.button("Popup")?;
    let popup = menu.popup()?;
    popup.text("Popup content")?;
    let close = popup.clone();
    popup.button("Close")?.on_click(move |_| close.hide())?;
    menu.on_click(move |_| match popup.is_shown()? {
        true => popup.hide(),
        false => popup.show(),
    })?;

    let toggles = column.row()?;
    let check = toggles.check_box("Check box", true)?;
    let mixed = toggles.check_box("Mixed", true)?;
    mixed.set_mixed(true)?;
    let switch = toggles.switch("Switch", false)?;
    let shown = status.clone();
    check.on_change(move |check| shown.set_text(&format!("Checked: {}", check.is_checked()?)))?;
    let shown = status.clone();
    switch.on_change(move |switch| shown.set_text(&format!("Switch: {}", switch.is_checked()?)))?;
    let radios = column.row()?;
    for (text, checked) in [("Small", false), ("Medium", true), ("Large", false)] {
        let shown = status.clone();
        radios
            .radio(text, checked)?
            .on_change(move |_| shown.set_text(&format!("Size: {text}")))?;
    }

    column.separator()?;
    let slider = column.slider(0.0, 100.0, 60.0)?;
    slider.set_accessible_label("Progress")?;
    let progress = column.progress(0.0, 100.0, 60.0)?;
    let numbers = column.row()?;
    numbers.set_align_items(Some(Align::Center))?;
    let amount = numbers.number_field(0.0, 100.0, 60.0)?;
    amount.set_accessible_label("Amount")?;
    amount.set_width(96.0)?;
    amount.set_tooltip(Some("Type a value or use the steppers"))?;
    numbers.separator()?;
    let busy = numbers.progress(0.0, 1.0, 0.0)?;
    busy.set_indeterminate(true)?;
    busy.set_grow(1.0)?;
    let level = numbers.slider(0.0, 10.0, 4.0)?;
    level.set_orientation(Orientation::Vertical)?;
    level.set_height(88.0)?;
    level.set_accessible_label("Level")?;
    // The slider and the number field drive the same progress value.
    let (bar, field) = (progress.clone(), amount.clone());
    slider.on_change(move |slider| {
        bar.set_value(slider.value()?)?;
        field.set_value(slider.value()?)
    })?;
    amount.on_change(move |amount| {
        progress.set_value(amount.value()?)?;
        slider.set_value(amount.value()?)
    })?;
    column.separator()?;

    let themes = column.row()?;
    themes.set_align_items(Some(Align::Center))?;
    themes.text("Theme")?;
    let theme = themes.dropdown(&THEMES, 0)?;
    theme.set_grow(1.0)?;
    let window = window.clone();
    theme.on_change(move |theme| {
        window.set_theme(match theme.selected()? {
            0 => Theme::light(),
            1 => Theme::dark(),
            _ => Theme::high_contrast(),
        })
    })?;
    Ok(())
}

/// Image, canvas and the scrolling family on one tab, a table on another.
fn views(pane: &Container) -> Result {
    let tabs = pane.tabs()?;
    tabs.set_grow(1.0)?;
    tabs.set_min_height(0.0)?;
    let column = tabs.add("Views")?;
    let data = tabs.add("Table")?;
    for page in [&column, &data] {
        page.set_gap(10.0)?;
    }
    let media = column.row()?;
    let pixels = (0..48 * 48)
        .flat_map(|i| [(i % 48 * 5) as u8, (i / 48 * 5) as u8, 160, 255])
        .collect();
    media.image(&Image::new(48, 48, pixels)?)?;
    let star = star()?;
    let turn = Rc::new(Cell::new(0.0f32));
    let angle = turn.clone();
    let canvas = media.canvas(move |builder, size| {
        let (sin, cos) = angle.get().sin_cos();
        let center = Affine::translation(size.width / 2.0, size.height / 2.0)?;
        builder.push_transform(Affine::new([cos, sin, -sin, cos, 0.0, 0.0])?.then(center)?)?;
        builder.fill_path(&star, Color::rgb(53, 92, 218))?;
        builder.stroke_path(&star, Color::rgb(32, 36, 43), Stroke::new(1.5))?;
        builder.pop()?;
        Ok(())
    })?;
    canvas.set_size(Some(48.0), Some(48.0))?;
    media.button("Rotate")?.on_click(move |_| {
        turn.set(turn.get() + 0.3);
        canvas.invalidate()
    })?;
    // Native effects: a shadow beneath a gradient background, on every backend.
    let card = media.column()?;
    card.set_size(Some(112.0), Some(48.0))?;
    card.set_radius(8.0)?;
    card.set_justify_content(Some(Justify::Center))?;
    card.set_align_items(Some(Align::Center))?;
    let stop = |offset, color| GradientStop { offset, color };
    let stops = [
        stop(0.0, Color::rgb(255, 120, 80)),
        stop(1.0, Color::rgb(90, 60, 220)),
    ];
    let gradient = Gradient::linear(Point::new(0.0, 0.0), Point::new(1.0, 1.0), &stops)?;
    card.set_background_gradient(Some(gradient))?;
    card.set_shadow(Some(Shadow {
        offset: Point::new(0.0, 4.0),
        blur: 6.0,
        spread: 0.0,
        color: Color::rgba(0, 0, 0, 110),
    }))?;
    card.text("Gradient")?.set_foreground(Color::WHITE)?;

    let lists = column.row()?;
    let scroll = lists.scroll_view()?;
    scroll.set_size(None, Some(120.0))?;
    scroll.set_grow(1.0)?;
    for i in 1..=12 {
        scroll.text(&format!("Scroll item {i}"))?;
    }
    let list = lists.list_view(26.0, 10_000, |row, index| {
        row.text(&format!("Row {index}")).map(drop)
    })?;
    list.set_size(None, Some(120.0))?;
    list.set_grow(1.0)?;
    let words = ["Short row.", "A longer row that wraps onto a second line."];
    let variable = column.variable_list_view(24.0, 200, move |row, index| {
        row.set_padding(4.0)?;
        row.text(&format!("{index}. {}", words[index % 2]))
            .map(drop)
    })?;
    variable.set_height(Some(110.0))?;

    let columns = [
        TableColumn {
            title: "Name",
            width: Some(140.0),
        },
        TableColumn {
            title: "Size",
            width: Some(80.0),
        },
        TableColumn {
            title: "Kind",
            width: None,
        },
    ];
    let table = data.table(&columns, 26.0, 1_000, |cell, row, column| {
        let text = match column {
            0 => format!("file-{row}.txt"),
            1 => format!("{} KB", row * 3 + 1),
            _ => "Text".into(),
        };
        cell.text(&text).map(drop)
    })?;
    table.set_grow(1.0)?;
    // Share only the space left, instead of starting from all 1,000 rows.
    table.set_basis(0.0)?;
    Ok(())
}

fn main() -> Result<()> {
    // `showcase vulkan` or `showcase wgpu` selects a GPU backend built in, and
    // a trailing `rtl` lays the window out right to left.
    let rtl = std::env::args().any(|arg| arg == "rtl");
    let renderer = match std::env::args().nth(1).as_deref() {
        #[cfg(feature = "vulkan")]
        Some("vulkan") => RendererBackend::Vulkan,
        #[cfg(feature = "wgpu")]
        Some("wgpu") => RendererBackend::Wgpu,
        _ => RendererBackend::default(),
    };
    let app = App::with_options(AppOptions {
        renderer,
        ..Default::default()
    })?;
    let options = WindowOptions {
        width: 960,
        height: 720,
        ..Default::default()
    };
    let window = app.window_with_options("Aegle — all controls", options)?;
    window.set_padding(16.0)?;
    window.set_gap(12.0)?;
    if rtl {
        window.set_layout_direction(Some(LayoutDirection::Rtl))?;
    }
    let panes = window.splitter(Orientation::Horizontal)?;
    panes.set_ratio(0.48)?;
    let (left, right) = (panes.first(), panes.second());
    // Insets are physical: each pane keeps its gap on the splitter's side.
    let (inner, outer) = (
        Insets::new(0.0, 12.0, 0.0, 0.0),
        Insets::new(0.0, 0.0, 0.0, 12.0),
    );
    let (first, second) = if rtl { (outer, inner) } else { (inner, outer) };
    left.set_padding(first)?;
    right.set_padding(second)?;
    for pane in [left, right] {
        pane.set_gap(10.0)?;
    }
    let status = window.text("Interact with any control.")?;
    inputs(&window, left, &status)?;
    views(right)?;
    app.run()
}
