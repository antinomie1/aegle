//! Reactive Rust state: controls follow signals, and input controls write the
//! user's changes back, so the code below changes the model only.
use aegle::prelude::*;

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

    // Reads three signals; it reruns when any of them changes.
    let (who, level) = (name.clone(), volume.clone());
    muted.bind(&window.text(""), move |summary, muted| {
        let volume = if *muted { 0.0 } else { level.get() };
        summary.set_text(&format!("你好，{} — volume {volume:.0}", who.get()));
    });

    let row = window.row();
    let reset = (name.clone(), volume.clone(), muted.clone());
    row.button("Reset").on_click(move |_| {
        reset.0.set("世界".into());
        reset.1.set(40.0);
        reset.2.set(false);
    });
    row.button("Louder")
        .on_click(move |_| volume.update(|v| *v = (*v + 10.0).min(100.0)));
    app.run()
}
