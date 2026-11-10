//! Reactive Rust state: controls follow signals and values derived from them,
//! and input controls write the user's changes back, so the code below
//! changes the model only.
use aegle::{prelude::*, ui::batch};

fn main() -> Result<()> {
    let app = App::new()?;
    let window = app.window("Aegle — signals")?;
    window.set_padding(16.0);
    window.set_gap(8.0, 8.0);

    let name = Signal::new(String::from("世界"));
    let volume = Signal::new(40.0);
    let muted = Signal::new(false);

    window.text_field("").bind_text(&name);
    window.slider(0.0, 100.0, 0.0).bind_value(&volume);
    window.check_box("Muted", false).bind_checked(&muted);

    // Computed from two signals; its readers rerun only when it changes, so
    // moving the slider while muted leaves the summary alone.
    let (quiet, level) = (muted.clone(), volume.clone());
    let audible = Derived::new(move || if quiet.get() { 0.0 } else { level.get() });
    let who = name.clone();
    audible.bind(&window.text(""), move |summary, volume| {
        summary.set_text(&format!("你好，{} — volume {volume:.0}", who.get()));
    });

    let row = window.row();
    let reset = (name.clone(), volume.clone(), muted.clone());
    // One pass for the three changes: the summary is set once.
    row.button("Reset").on_click(move |_| {
        batch(|| {
            reset.0.set("世界".into());
            reset.1.set(40.0);
            reset.2.set(false);
        })
    });
    row.button("Louder")
        .on_click(move |_| volume.update(|v| *v = (*v + 10.0).min(100.0)));
    app.run()
}
