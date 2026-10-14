//! Gradient backgrounds tween under the paint timing: from the background
//! color, between stops and geometries, and back, sampling each frame into
//! the same stop storage without allocating.
#![cfg(feature = "motion")]
// A counting global allocator, as in the `thousand` example, is unsafe code.
#![allow(unsafe_code)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::RefCell,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use aegle_ui::{
    Color, Easing, Point, Result, Size, TextSystem, Theme, Transition, TransitionProperty, Ui,
    scene::{Command, Gradient, GradientGeometry, GradientStop},
};

struct Counting;
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwards the caller's layout to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `alloc` with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// The drawn gradient, if any.
fn drawn(ui: &Ui) -> Result<Option<Gradient>> {
    ui.refresh()?;
    let mut found = None;
    ui.visit_scenes(|visit| {
        if let aegle_ui::Visit::Scene { scene, .. } = visit
            && scene
                .commands()
                .iter()
                .any(|c| matches!(c, Command::FillGradient { .. }))
        {
            found = scene.gradients().first().cloned();
        }
        Ok(())
    })?;
    Ok(found)
}

/// Allocations of advancing and refreshing five frames 10 ms apart from `from` ms.
fn frames(ui: &Ui, from: u64) -> Result<usize> {
    let before = ALLOCATIONS.load(Ordering::Relaxed);
    for frame in 0..5 {
        ui.advance_animations(Duration::from_millis(from + frame * 10))?;
        ui.refresh()?;
    }
    Ok(ALLOCATIONS.load(Ordering::Relaxed) - before)
}

#[test]
fn gradients_tween_without_allocating_per_frame() -> Result {
    let ui = Ui::with_fonts(Rc::new(RefCell::new(TextSystem::new())), Theme::light())?;
    ui.resize(Size::new(200.0, 100.0));
    let panel = ui.root().column();
    panel.set_width(80.0);
    panel.set_height(40.0);
    panel.set_background(Color::rgb(0, 0, 0));
    let linear = Transition::new(Duration::from_millis(100), Easing::Linear);
    panel.set_property_transition(TransitionProperty::Paint, linear);
    ui.refresh()?;
    let stop = |offset, color| GradientStop { offset, color };
    let down = Gradient::linear(
        Point::new(0.5, 0.0),
        Point::new(0.5, 1.0),
        &[stop(0.0, Color::WHITE), stop(1.0, Color::WHITE)],
    )?;

    // Appearing: from the black background toward white.
    panel.set_background_gradient(down.clone());
    assert_eq!(panel.background_gradient(), Some(down.clone()));
    ui.advance_animations(Duration::ZERO)?;
    drawn(&ui)?;
    ui.advance_animations(Duration::from_millis(50))?;
    let half = drawn(&ui)?.expect("a gradient is drawn");
    let [r, ..] = half.stops()[0].color.to_rgba();
    assert!((100..250).contains(&r), "{r}");
    ui.advance_animations(Duration::from_millis(100))?;
    let geometry = drawn(&ui)?.unwrap().geometry();
    assert_eq!(
        geometry,
        GradientGeometry::Linear {
            start: Point::new(40.0, 0.0),
            end: Point::new(40.0, 40.0),
        }
    );
    assert!(!panel.is_animating());

    // Between gradients the direction turns and stops are padded.
    let across = Gradient::linear(
        Point::new(0.0, 0.5),
        Point::new(1.0, 0.5),
        &[
            stop(0.0, Color::BLACK),
            stop(0.5, Color::WHITE),
            stop(1.0, Color::BLACK),
        ],
    )?;
    panel.set_background_gradient(across.clone());
    ui.advance_animations(Duration::from_millis(100))?;
    drawn(&ui)?;
    let tween = frames(&ui, 110)?;
    let middle = drawn(&ui)?.unwrap();
    assert_eq!(middle.stops().len(), 3);
    let GradientGeometry::Linear { start, end } = middle.geometry() else {
        panic!("linear stays linear");
    };
    // Turned partway from down to across.
    assert!(end.x > start.x && end.y > start.y, "{start:?} {end:?}");

    // Sampling reuses the stop storage the last frame's records released.
    assert_eq!(tween, 0);

    // Going: back to the background color, then no gradient at all.
    ui.advance_animations(Duration::from_millis(300))?;
    assert_eq!(
        drawn(&ui)?,
        Some(across.with_geometry(GradientGeometry::Linear {
            start: Point::new(0.0, 20.0),
            end: Point::new(80.0, 20.0),
        })?)
    );
    panel.set_background_gradient(None);
    assert_eq!(panel.background_gradient(), None);
    ui.refresh()?;
    ui.advance_animations(Duration::from_millis(350))?;
    assert!(drawn(&ui)?.is_some());
    ui.advance_animations(Duration::from_millis(400))?;
    assert!(drawn(&ui)?.is_none());
    Ok(())
}
