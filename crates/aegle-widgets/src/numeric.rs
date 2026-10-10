//! Slider and progress handles; their controls are in `range_control`.

use aegle_controls::{Input, Range, RangeError};
use aegle_core::Dirty;
use aegle_layout::Style;
use aegle_ui::OrFail;
use aegle_ui::{Container, Control, HandlerResult, Node, UiError, handle};

use crate::range_control::{Orientation, ProgressControl, SliderControl, sized};

handle! {
    /// A numeric slider with pointer, keyboard, wheel (while focused) and semantic adjustment.
    pub Slider(SliderControl): interactive, pressed, indicator
}
handle! {
    /// A determinate or indeterminate progress indicator; it is not focusable.
    pub Progress(ProgressControl): indicator
}

macro_rules! ranges {
    ($($ty:ident),*) => { $(impl $ty {
        /// Returns the retained numeric value.
        pub fn value(&self) -> f64 { read_range(&self.0, |range| range.value()) }
        /// Returns the inclusive minimum and maximum.
        pub fn range(&self) -> (f64, f64) { read_range(&self.0, |range| (range.min(), range.max())) }
        /// Sets a finite value, clamped to the bounds and slider step grid.
        /// Does not invoke the user-change callback.
        pub fn set_value(&self, value: f64) {
            update_range(&self.0, |range| range.set_value(value))
        }
        /// Replaces finite increasing bounds with finite span, then clamps the
        /// current value. Panics on invalid bounds.
        pub fn set_range(&self, min: f64, max: f64) {
            update_range(&self.0, |range| range.set_bounds(min, max))
        }
    })* };
}
ranges!(Slider, Progress);

impl Slider {
    /// Sets a finite nonnegative step anchored at the minimum; zero is continuous.
    /// The maximum is always reachable, including when it is not on the step grid.
    pub fn set_step(&self, step: f64) {
        update_range(&self.0, |range| range.set_step(step))
    }
    /// Returns the step; zero means continuous pointer adjustment.
    pub fn step(&self) -> f64 {
        read_range(&self.0, |range| range.step())
    }
    /// Requests one enabled user increment, including a change callback if changed.
    pub fn increment(&self) {
        self.change(|state, id| state.dispatch(id, Input::Increment))
    }
    /// Requests one enabled user decrement, including a change callback if changed.
    pub fn decrement(&self) {
        self.change(|state, id| state.dispatch(id, Input::Decrement))
    }
    /// Adds a user-change handler; handlers run in registration order outside tree borrows. The handle
    /// exposes the latest value; pending notifications are not value snapshots.
    pub fn on_change<R: HandlerResult>(&self, mut callback: impl FnMut(Self) -> R + 'static) {
        self.on_action(move |node| callback(Self(node)).into_result())
    }
}

fn read_range<T>(node: &Node, read: impl FnOnce(&Range) -> T) -> T {
    node.change(|state, id| {
        let control = &mut state.tree.get_mut(id).unwrap().context.control;
        let range = if let Some(slider) =
            (&mut **control as &mut dyn std::any::Any).downcast_mut::<SliderControl>()
        {
            slider.behavior.range()
        } else if let Some(progress) =
            (&mut **control as &mut dyn std::any::Any).downcast_mut::<ProgressControl>()
        {
            &progress.range
        } else {
            return Err(UiError::WrongKind.into());
        };
        Ok(read(range))
    })
}

/// Applies a programmatic range change; a changed value eases into place
/// unless changed inside `Node::snap`.
fn update_range(
    node: &Node,
    update: impl FnOnce(&mut Range) -> std::result::Result<bool, RangeError>,
) {
    node.change(|state, id| {
        #[cfg(feature = "motion")]
        let jump = state.snapping(id);
        #[cfg(not(feature = "motion"))]
        let jump = true;
        let control = &mut state.tree.get_mut(id).unwrap().context.control;
        let changed = if let Some(slider) =
            (&mut **control as &mut dyn std::any::Any).downcast_mut::<SliderControl>()
        {
            let changed = update(slider.behavior.range_mut())?;
            let fraction = slider.behavior.range().fraction();
            slider.glide.retarget(fraction, jump);
            changed
        } else if let Some(progress) =
            (&mut **control as &mut dyn std::any::Any).downcast_mut::<ProgressControl>()
        {
            let changed = update(&mut progress.range)?;
            progress.glide.retarget(progress.range.fraction(), jump);
            changed
        } else {
            return Err(UiError::WrongKind.into());
        };
        if changed {
            state.tree.mark_dirty(id, Dirty::PAINT | Dirty::SEMANTICS)?;
        }
        Ok(())
    })
}

/// Switches orientation and the themed cross-axis size that goes with it.
fn orient(node: &Node, orientation: Orientation) {
    node.change(|state, id| {
        let vertical = orientation == Orientation::Vertical;
        let theme = *state.theme_of(id);
        let element = &mut state.tree.get_mut(id).unwrap().context;
        let local = element.local_layout;
        let control: &mut dyn std::any::Any = &mut *element.control;
        let extent = if let Some(slider) = control.downcast_mut::<SliderControl>() {
            slider.vertical = vertical;
            theme.control_height
        } else if let Some(progress) = control.downcast_mut::<ProgressControl>() {
            progress.vertical = vertical;
            theme.control_height / 2.0
        } else {
            return Err(UiError::WrongKind.into());
        };
        let mut style = state.tree.get(id).unwrap().style().clone();
        if !local.contains(aegle_ui::LocalLayout::HEIGHT) {
            sized(&mut style, extent, vertical);
        }
        aegle_layout::set_style(&mut state.tree, id, style)?;
        Ok(())
    });
}

impl Progress {
    /// Lays the bar out from bottom to top or left to right.
    pub fn set_orientation(&self, orientation: Orientation) {
        orient(&self.0, orientation)
    }
    /// Shows ongoing work of unknown length: a sweeping segment (a still one
    /// with reduced motion) and no numeric value for assistive technology.
    pub fn set_indeterminate(&self, indeterminate: bool) {
        self.update(|progress| {
            progress.indeterminate = indeterminate;
            progress.sweep = None;
        })
    }
    /// Whether the bar shows indeterminate progress.
    pub fn is_indeterminate(&self) -> bool {
        self.read(|progress| progress.indeterminate)
    }
}

impl Slider {
    /// Lays the slider out from bottom to top or left to right; Up/Right and
    /// wheel up increase it either way.
    pub fn set_orientation(&self, orientation: Orientation) {
        orient(&self.0, orientation)
    }
}

pub(crate) fn slider(container: &Container, min: f64, max: f64, value: f64) -> Slider {
    let range = Range::new(min, max, value, 0.0).or_fail();
    Slider(crate::add(container, |_, theme| {
        let mut style = Style {
            flex_shrink: 0.0,
            ..Default::default()
        };
        sized(&mut style, theme.control_height, false);
        Ok((
            Box::new(SliderControl::new(range)) as Box<dyn Control>,
            style,
        ))
    }))
}

pub(crate) fn progress(container: &Container, min: f64, max: f64, value: f64) -> Progress {
    let range = Range::new(min, max, value, 0.0).or_fail();
    Progress(crate::add(container, |_, theme| {
        let mut style = Style {
            flex_shrink: 0.0,
            ..Default::default()
        };
        sized(&mut style, theme.control_height / 2.0, false);
        Ok((
            Box::new(ProgressControl::new(range)) as Box<dyn Control>,
            style,
        ))
    }))
}
