//! Small reusable component factories share native control behavior and semantics.
use aegle::prelude::*;
use std::time::Duration;

fn action_button(parent: &Container, text: &str) -> Result<Button> {
    let button = parent.button(text)?;
    button.set_skin(primary)?;
    button.set_padding(18.0)?;
    button.set_transition(Transition::new(Duration::from_millis(120), Easing::EaseOut))?;
    Ok(button)
}

fn primary(theme: &Theme, state: VisualState) -> Appearance {
    // A component library owns its palette; input, focus, IME and accessibility
    // remain in Aegle. A complete MD3 library would also provide more behaviors.
    let mut look = Appearance::new(theme, state);
    look.radius = 18.0;
    look.border_width = 0.0;
    if state.enabled {
        look.background = if state.pressed {
            Color::rgb(66, 45, 103)
        } else if state.hovered {
            Color::rgb(91, 68, 130)
        } else {
            Color::rgb(103, 80, 164)
        };
        look.foreground = Color::WHITE;
    }
    look
}

fn card(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        background: theme.surface,
        ..Appearance::new(theme, state)
    }
}

fn main() -> Result<()> {
    let app = App::new()?;
    let view = aegle::ui!(&app, "examples/components.aegle")?;
    view.card.set_skin(card)?;
    action_button(&view.actions, "Clear text")?.on_click(move |_| view.editor.set_text(""))?;
    let window = view.root.clone();
    action_button(&view.actions, "Dark theme")?
        .on_click(move |_| window.set_theme(Theme::dark()))?;
    action_button(&view.actions, "Close")?.on_click(move |_| view.root.close())?;
    app.run()
}
