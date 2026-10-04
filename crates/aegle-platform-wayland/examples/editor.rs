//! Native Wayland CJK editor; no GPU, window toolkit or polling timer required.
//!
//! Optional `AEGLE_SMOKE_SECONDS=3` exits after a bounded compositor smoke run.
//! This example uses a tiny licensed font fixture with intentionally limited
//! coverage. A desktop application should register its chosen system fonts.

mod editor_support;

use aegle_platform_wayland::{
    Event, ImeCause, ImeEvent, PixelSize, PointerEventKind, Wayland, WindowOptions,
};
use aegle_render_software::{Renderer, Surface};
use aegle_scene::{Affine, Color, Point, Rect, RoundedRect, Scene};
use aegle_text::{
    Alignment, Blob, EditorOptions, EditorPaint, HitSelection, TextStyle, TextSystem,
};
use editor_support::Result;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

fn main() -> Result<()> {
    let mut backend = Wayland::connect()?;
    let window = backend.create_window(WindowOptions {
        title: "Aegle — retained CJK editor",
        size: PixelSize {
            width: 640,
            height: 360,
        },
        ..Default::default()
    })?;
    let mut fonts = TextSystem::new();
    fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?;
    let style = TextStyle {
        families: "Aegle Test CJK",
        size: 23.0,
        color: Color::rgb(35, 48, 71),
        ..Default::default()
    };
    let mut editor = fonts.editor(
        "Hello, 世界\n你好 / 日本語 / 한글",
        &style,
        EditorOptions {
            multiline: true,
            ..Default::default()
        },
    )?;
    let heading = fonts.paragraph(
        "Aegle / retained native editing",
        &TextStyle {
            size: 16.0,
            ..style.clone()
        },
    )?;
    let help = fonts.paragraph(
        "Type, drag to select, Ctrl+A, Ctrl+Z / Ctrl+Shift+Z",
        &TextStyle {
            size: 12.0,
            color: Color::rgb(92, 105, 126),
            ..style
        },
    )?;
    let origin = Point::new(40.0, 88.0);
    let mut renderer = Renderer::default();
    let mut scene = Scene::default();
    let mut focused = false;
    let mut dragging = false;
    let mut scroll = 0.0;
    let mut dirty = true;
    let mut ime_sync = true;
    let mut cause = ImeCause::Other;
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
        let mut keep_caret_visible = false;
        while let Some(event) = backend.next_event() {
            match event {
                Event::Close { .. } => {
                    backend.remove_window(window)?;
                    return Ok(());
                }
                Event::Configure { info, .. } => {
                    fonts.edit(&mut editor).reflow(
                        Some((info.size.width as f32 - 80.0).max(1.0)),
                        Alignment::Start,
                    )?;
                    dirty = true;
                    ime_sync = true;
                    keep_caret_visible = true;
                }
                Event::Redraw { .. } => ready = true,
                Event::KeyboardFocus { focused: next, .. } => {
                    focused = next;
                    dragging = false;
                    editor.break_undo_group();
                    if !focused {
                        fonts.edit(&mut editor).cancel_preedit();
                    }
                    dirty = true;
                    ime_sync = true;
                    cause = ImeCause::Other;
                }
                Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if editor_support::key(&mut fonts, &mut editor, key, modifiers)? {
                        ime_sync = true;
                        cause = ImeCause::Other;
                        keep_caret_visible = true;
                    }
                }
                Event::Pointer { position, kind, .. } => match kind {
                    PointerEventKind::Press { button: 0x110, .. } => {
                        fonts.edit(&mut editor).cancel_preedit();
                        fonts.edit(&mut editor).select_at(
                            Point::new(position.x - origin.x, position.y - origin.y + scroll),
                            HitSelection::Caret,
                        )?;
                        dragging = true;
                        ime_sync = true;
                        cause = ImeCause::Other;
                    }
                    PointerEventKind::Motion { .. } if dragging => {
                        fonts.edit(&mut editor).select_at(
                            Point::new(position.x - origin.x, position.y - origin.y + scroll),
                            HitSelection::Extend,
                        )?;
                        ime_sync = true;
                        cause = ImeCause::Other;
                    }
                    PointerEventKind::Release { button: 0x110, .. }
                    | PointerEventKind::Leave { .. } => dragging = false,
                    PointerEventKind::Axis { vertical, .. } => {
                        scroll = (scroll + vertical.absolute as f32).max(0.0);
                        dirty = true;
                        ime_sync = true;
                    }
                    _ => {}
                },
                Event::Ime {
                    event: ImeEvent::Update(update),
                    ..
                } => {
                    editor_support::apply_ime(&mut fonts, &mut editor, update)?;
                    ime_sync = true;
                    cause = ImeCause::InputMethod;
                    keep_caret_visible = true;
                }
                Event::Ime {
                    event: ImeEvent::Left,
                    ..
                } => {
                    fonts.edit(&mut editor).cancel_preedit();
                    dirty = true;
                    ime_sync = true;
                    cause = ImeCause::Other;
                }
                Event::Ime {
                    event: ImeEvent::Entered,
                    ..
                } => {
                    ime_sync = true;
                }
                Event::Error(error) => return Err(error.into()),
                _ => {}
            }
        }
        let info = backend.window_info(window)?;
        let viewport_height = (info.size.height as f32 - 142.0).max(1.0);
        if keep_caret_visible {
            let caret = editor.ime_rect();
            scroll = scroll.min(caret.origin.y);
            scroll = scroll.max(caret.origin.y + caret.size.height - viewport_height);
        }
        scroll = scroll.clamp(0.0, (editor.size().height - viewport_height).max(0.0));
        let changes = editor.take_changes();
        dirty |= changes.value || changes.layout || changes.selection;
        if ime_sync && backend.ime_available() {
            // All queued transactions are applied before replying. The backend
            // retains this desired state while an older serial is outstanding.
            backend.configure_ime(
                window,
                Some(editor_support::request(&editor, origin, scroll, cause)?),
            )?;
            ime_sync = false;
            cause = ImeCause::InputMethod;
        }
        if dirty {
            let mut builder = std::mem::take(&mut scene).into_builder();
            builder.clear();
            builder.push_transform(Affine::translation(24.0, 22.0)?)?;
            heading.paint(&mut builder)?;
            builder.pop()?;
            let field = RoundedRect::new(
                Rect::new(
                    24.0,
                    72.0,
                    (info.size.width as f32 - 48.0).max(1.0),
                    viewport_height + 32.0,
                ),
                10.0,
            )?;
            builder.fill(field, Color::WHITE)?;
            builder.stroke(
                field,
                if focused {
                    Color::rgb(94, 127, 189)
                } else {
                    Color::rgb(192, 202, 218)
                },
                1.0,
            )?;
            builder.push_clip(field)?;
            builder.push_transform(Affine::translation(origin.x, origin.y - scroll)?)?;
            editor.paint(
                &mut builder,
                EditorPaint {
                    caret: focused.then_some(Color::rgb(50, 91, 166)),
                    preedit: Some(Color::rgb(50, 91, 166)),
                    ..Default::default()
                },
            )?;
            builder.pop()?.pop()?;
            builder.push_transform(Affine::translation(24.0, info.size.height as f32 - 28.0)?)?;
            help.paint(&mut builder)?;
            builder.pop()?;
            scene = builder.finish()?;
            backend.request_redraw(window)?;
            dirty = false;
        }
        if ready {
            let scale = Affine::scale(info.scale as f32, info.scale as f32)?;
            if backend.present(window, |pixels, size| {
                let mut surface = Surface::new(pixels, size.width, size.height)?;
                renderer
                    .begin_frame(&mut surface, Color::rgb(239, 242, 247))
                    .draw(&scene, scale)
            })? {
                presented += 1;
                if presented == 1 {
                    println!(
                        "first Wayland frame presented; IME={}",
                        backend.ime_available()
                    );
                }
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
