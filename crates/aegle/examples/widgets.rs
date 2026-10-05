//! Binary and numeric controls share retained values, CJK input and native semantics.
use aegle::prelude::*;

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/widgets.aegle")?;
    let (switch, editor) = (view.edit_switch.clone(), view.editor.clone());
    view.edit_check.on_change(move |check| {
        let checked = check.is_checked()?;
        switch.set_checked(checked)?;
        editor.set_enabled(checked)
    })?;
    let (check, editor) = (view.edit_check.clone(), view.editor.clone());
    view.edit_switch.on_change(move |switch| {
        let checked = switch.is_checked()?;
        check.set_checked(checked)?;
        editor.set_enabled(checked)
    })?;
    view.slider.on_change(move |slider| {
        let value = slider.value()?;
        view.progress.set_value(value)?;
        view.amount.set_text(&format!("进度：{value:.0}%"))
    })?;
    let window = view.root.clone();
    view.light
        .on_click(move |_| window.set_theme(Theme::light()))?;
    let window = view.root.clone();
    view.dark
        .on_click(move |_| window.set_theme(Theme::dark()))?;
    view.close.on_click(move |_| view.root.close())?;
    app.run()
}
