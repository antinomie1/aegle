//! Measures a 1000-control headless scene: default controls, a custom
//! control kind and skins, with a counting allocator. Prints the first frame
//! (build, layout, paint) and steady refreshes: idle, one value change and a
//! hover move. Run in release:
//!
//! `cargo run -p aegle-widgets --example thousand --release`

// The counting allocator forwards to `System`, which needs unsafe.
#![allow(unsafe_code)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use aegle_text::{Blob, GenericFamily, TextSystem};
use aegle_ui::{
    Appearance, Color, Container, Control, ControlKind, Modifiers, Point, PointerId, PointerKind,
    Result, Size, Theme, Ui, VisualState, container_style,
    control::{Frame, MeasureCx, PaintCx},
    handle,
    scene::{Rect, RoundedRect},
};
use aegle_widgets::Widgets;

struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwards the caller's layout contract to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by `System` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: as for `dealloc`, with the caller's new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Five cells, the custom control kind of the scene.
struct Meter {
    value: u8,
}

impl Control for Meter {
    fn kind(&self) -> ControlKind {
        ControlKind::Slider
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
    fn measure(&mut self, _: &MeasureCx<'_>) -> Result<Size> {
        Ok(Size::new(5.0 * 12.0, 12.0))
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) -> Result {
        for step in 0..5u8 {
            let cell = RoundedRect::new(Rect::new(f32::from(step) * 12.0, 0.0, 10.0, 10.0), 2.0)?;
            let color = if step < self.value {
                cx.appearance.indicator
            } else {
                cx.appearance.border_color
            };
            cx.builder.fill(cell, color)?;
        }
        Ok(())
    }
}

handle! {
    /// A meter.
    MeterHandle(Meter)
}

impl MeterHandle {
    fn new(parent: &Container, value: u8) -> Result<Self> {
        parent
            .add(|_, theme| Ok((Box::new(Meter { value }), container_style(theme, false))))
            .map(Self)
    }
    fn set_value(&self, value: u8) -> Result {
        self.update(|meter| meter.value = value)
    }
}

fn meter_skin(theme: &Theme, state: VisualState) -> Appearance {
    Appearance {
        indicator: Color::rgb(230, 160, 0),
        ..Appearance::new(theme, state)
    }
}

fn counted<T>(run: impl FnOnce() -> Result<T>) -> Result<(T, Duration, usize)> {
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    let start = Instant::now();
    let value = run()?;
    let time = start.elapsed();
    Ok((value, time, ALLOCATIONS.load(Ordering::Relaxed) - before))
}

fn frame(ui: &Ui) -> Result {
    ui.refresh()?;
    ui.visit_scenes(|_| Ok(()))
}

fn main() -> Result {
    let mut text = TextSystem::new();
    let font = include_bytes!("../../../tests/assets/aegle-test-cjk.otf");
    let families = text.register_fonts(Blob::new(Arc::new(font.as_slice())))?;
    let ids = families.iter().map(|(id, _)| *id);
    text.collection_mut()
        .set_generic_families(GenericFamily::SansSerif, ids);
    let fonts = Rc::new(RefCell::new(text));

    let (ui, meters, buttons) = {
        let (built, time, allocations) = counted(|| {
            let ui = Ui::with_fonts(fonts.clone(), Theme::light())?;
            let list = ui.root().scroll_view()?;
            let (mut meters, mut buttons) = (Vec::new(), Vec::new());
            for row in 0..125 {
                let line = list.row()?;
                line.text(&format!("行 {row}"))?;
                buttons.push(line.button("Open")?);
                line.check_box("Done", row % 2 == 0)?;
                line.slider(0.0, 1.0, 0.5)?;
                for value in 0..4 {
                    let meter = MeterHandle::new(&line, value)?;
                    meter.set_skin(meter_skin)?;
                    meters.push(meter);
                }
            }
            ui.resize(Size::new(1280.0, 800.0))?;
            frame(&ui)?;
            Ok((ui, meters, buttons))
        })?;
        println!("first frame: {time:?}, {allocations} allocations");
        built
    };

    const RUNS: u32 = 200;
    let (_, time, allocations) = counted(|| {
        for _ in 0..RUNS {
            frame(&ui)?;
        }
        Ok(())
    })?;
    println!(
        "idle refresh: {:?}, {} allocations",
        time / RUNS,
        allocations as f64 / f64::from(RUNS)
    );

    let (_, time, allocations) = counted(|| {
        for run in 0..RUNS {
            meters[run as usize % 16].set_value((run % 5) as u8)?;
            frame(&ui)?;
        }
        Ok(())
    })?;
    println!(
        "value change refresh: {:?}, {} allocations",
        time / RUNS,
        allocations as f64 / f64::from(RUNS)
    );

    let pointer = PointerId(1);
    let mut targets = Vec::new();
    for button in &buttons[..8] {
        let bounds = button.bounds()?;
        targets.push(Point::new(bounds.origin.x + 4.0, bounds.origin.y + 4.0));
    }
    let (_, time, allocations) = counted(|| {
        for run in 0..RUNS {
            let at = targets[run as usize % targets.len()];
            ui.pointer(pointer, PointerKind::Move, at, Modifiers::default())?;
            frame(&ui)?;
        }
        Ok(())
    })?;
    println!(
        "hover refresh: {:?}, {} allocations",
        time / RUNS,
        allocations as f64 / f64::from(RUNS)
    );
    Ok(())
}
