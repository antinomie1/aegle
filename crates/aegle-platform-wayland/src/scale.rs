//! wp-fractional-scale-v1 with wp-viewporter, for non-integer output scales.
//!
//! When both globals exist a surface keeps buffer scale 1, its viewport maps the
//! (rounded) physical buffer onto the logical size, and the compositor's
//! preferred scale arrives in 1/120 units. Without them the integer scale of the
//! surface's outputs applies, as before.

use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop, globals::GlobalList,
    protocol::wl_surface::WlSurface,
};
use wayland_protocols::wp::{
    fractional_scale::v1::client::{
        wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
        wp_fractional_scale_v1::{self, WpFractionalScaleV1},
    },
    viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use crate::{Event, State};

/// The bound globals, present only when both are available.
pub(crate) struct ScaleGlobals {
    manager: WpFractionalScaleManagerV1,
    viewporter: WpViewporter,
}

/// Per-surface objects; destroyed with the window.
pub(crate) struct Fractional {
    object: WpFractionalScaleV1,
    pub viewport: WpViewport,
}

impl Drop for Fractional {
    fn drop(&mut self) {
        self.object.destroy();
        self.viewport.destroy();
    }
}

impl ScaleGlobals {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<State>) -> Option<Self> {
        Some(Self {
            manager: globals.bind(qh, 1..=1, ()).ok()?,
            viewporter: globals.bind(qh, 1..=1, ()).ok()?,
        })
    }

    /// Requests preferred-scale events and a viewport for `surface`.
    pub fn attach(&self, surface: &WlSurface, qh: &QueueHandle<State>) -> Fractional {
        Fractional {
            object: self
                .manager
                .get_fractional_scale(surface, qh, surface.clone()),
            viewport: self.viewporter.get_viewport(surface, qh, ()),
        }
    }
}

delegate_noop!(State: ignore WpFractionalScaleManagerV1);
delegate_noop!(State: ignore WpViewporter);
delegate_noop!(State: ignore WpViewport);

impl Dispatch<WpFractionalScaleV1, WlSurface> for State {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let Some(window) = state
            .windows
            .iter_mut()
            .find(|w| w.window.wl_surface() == surface)
        else {
            return;
        };
        // Preferred scales run from 1/120 up; ignore a nonsensical zero.
        let scale = scale.max(120) as f32 / 120.0;
        if window.info.scale != scale {
            window.info.scale = scale;
            window.dirty = true;
            state.events.push_back(Event::Configure {
                window: window.id,
                info: window.info,
            });
        }
    }
}
