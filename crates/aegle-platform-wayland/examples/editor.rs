//! Native retained controls: one Taffy tree, routed input, focus and CJK/IME.
//!
//! Uses a tiny licensed font fixture; production apps choose their own fonts.
//! This example assigns a single logical focus domain to the active keyboard
//! seat. `AEGLE_SMOKE_SECONDS=3` bounds an isolated compositor run.
mod controls_support;

use aegle_controls::{Modifiers, PointerId, PointerKind};
use aegle_platform_wayland::{
    Event, ImeEvent, PixelSize, PointerEventKind, Wayland, WindowOptions,
};
use aegle_render_software::{Renderer, Surface};
use aegle_scene::{Affine, Color};
use controls_support::{App, Result};
use std::time::{Duration, Instant};
use wayland_client::Proxy;

fn main() -> Result<()> {
    let mut backend = Wayland::connect()?;
    let window = backend.create_window(WindowOptions {
        title: "Aegle — retained controls",
        app_id: "org.aegle.controls",
        size: PixelSize {
            width: 640,
            height: 420,
        },
        ..Default::default()
    })?;
    let mut app = App::new()?;
    #[cfg(feature = "example-accessibility")]
    let mut accessibility = {
        let wake = backend.wake_handle()?;
        aegle_access::UnixAdapter::new(move || wake.wake())
    };
    app.resize(backend.window_info(window)?.size)?;
    let mut renderer = Renderer::default();
    let mut presented = 0;
    let deadline = std::env::var("AEGLE_SMOKE_SECONDS")
        .ok()
        .map(|seconds| seconds.parse::<u64>())
        .transpose()?
        .map(|seconds| Instant::now() + Duration::from_secs(seconds));
    if !backend.ime_available() {
        eprintln!("text-input-v3 unavailable; compositor supports keyboard editing only");
    }
    loop {
        let mut ready = false;
        while let Some(event) = backend.next_event() {
            match event {
                Event::Close { .. } => {
                    backend.remove_window(window)?;
                    return Ok(());
                }
                Event::Configure { info, .. } => {
                    app.resize(info.size)?;
                    #[cfg(feature = "example-accessibility")]
                    accessibility.set_window_focused(info.active);
                }
                Event::Redraw { .. } => ready = true,
                Event::KeyboardFocus { seat, focused, .. } => {
                    if focused {
                        if app.seat.as_ref() != Some(&seat) {
                            app.window_focus(false)?;
                            app.seat = Some(seat);
                            app.modifiers = Modifiers::default();
                        }
                        app.window_focus(true)?;
                    } else if app.seat.as_ref() == Some(&seat) {
                        app.window_focus(false)?;
                        app.seat = None;
                    }
                }
                Event::Key {
                    seat,
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if app.seat.as_ref() == Some(&seat) => {
                    app.key(key, modifiers, pressed, repeat)?;
                }
                Event::Modifiers {
                    seat, modifiers, ..
                } if app.seat.as_ref() == Some(&seat) => {
                    app.modifiers = Modifiers {
                        shift: modifiers.shift,
                        control: modifiers.ctrl,
                        alt: modifiers.alt,
                        meta: modifiers.logo,
                    };
                }
                Event::Pointer {
                    seat,
                    position,
                    kind,
                    ..
                } => {
                    if app.seat.is_none() {
                        app.seat = Some(seat.clone());
                    }
                    if app.seat.as_ref() != Some(&seat) {
                        continue;
                    }
                    let pointer = PointerId(u64::from(seat.id().protocol_id()));
                    match kind {
                        PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                            app.pointer(pointer, PointerKind::Move, position)?
                        }
                        PointerEventKind::Press { button: 0x110, .. } => {
                            app.pointer(pointer, PointerKind::Down { clicks: 1 }, position)?
                        }
                        PointerEventKind::Release { button: 0x110, .. } => {
                            app.pointer(pointer, PointerKind::Up, position)?
                        }
                        PointerEventKind::Leave { .. } => app.pointer_leave()?,
                        PointerEventKind::Axis { vertical, .. } => {
                            app.scroll(position, vertical.absolute as f32)?
                        }
                        _ => {}
                    }
                }
                Event::Ime { seat, event, .. } if app.seat.as_ref() == Some(&seat) => match event {
                    ImeEvent::Update(update) => app.ime(update)?,
                    ImeEvent::Left => app.ime_left()?,
                    ImeEvent::Entered => app.ime_sync = true,
                },
                Event::Error(error) => return Err(error.into()),
                _ => {}
            }
            if std::mem::take(&mut app.ime_reset) && backend.ime_available() {
                backend.configure_ime(window, None)?;
            }
        }
        #[cfg(feature = "example-accessibility")]
        let mut initial_access = false;
        #[cfg(feature = "example-accessibility")]
        while let Some(event) = accessibility.next_event() {
            match event {
                aegle_access::Event::InitialTree => initial_access = true,
                aegle_access::Event::Action(action) => app.access_action(action)?,
                aegle_access::Event::Deactivate => {}
            }
        }
        if std::mem::take(&mut app.ime_reset) && backend.ime_available() {
            backend.configure_ime(window, None)?;
        }
        let info = backend.window_info(window)?;
        if app.refresh(info.size)? {
            backend.request_redraw(window)?;
        }
        #[cfg(feature = "example-accessibility")]
        if initial_access || app.access_dirty() {
            accessibility.update_if_active(|initial| app.accessibility(initial));
        }
        if std::mem::take(&mut app.ime_sync) && backend.ime_available() {
            backend.configure_ime(window, app.ime_request())?;
        }
        if ready
            && backend
                .present(window, |pixels, size| {
                    let mut surface = Surface::new(pixels, size.width, size.height)?;
                    let mut frame = renderer.begin_frame(&mut surface, Color::rgb(245, 246, 248));
                    let scale = info.scale as f32;
                    for (scene, origin) in app.records() {
                        frame.draw(
                            scene,
                            Affine::new([
                                scale,
                                0.0,
                                0.0,
                                scale,
                                origin.x * scale,
                                origin.y * scale,
                            ])?,
                        )?;
                    }
                    Ok::<_, Box<dyn std::error::Error>>(())
                })
                .map_err(|error| format!("present: {error}"))?
        {
            presented += 1;
            if presented == 1 {
                println!(
                    "first controls frame presented; IME={}",
                    backend.ime_available()
                );
            }
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            println!(
                "smoke complete: {presented} frames; glyph cache {:?}",
                renderer.glyph_cache().stats()
            );
            backend.remove_window(window)?;
            return Ok(());
        }
        backend.dispatch(
            deadline.map(|deadline| deadline.saturating_duration_since(Instant::now())),
        )?;
    }
}
