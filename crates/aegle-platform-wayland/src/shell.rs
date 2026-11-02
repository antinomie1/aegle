//! Surface roles beyond the ordinary toplevel: wlr layer surfaces and their
//! placement, xdg popups attached to any window, and the outputs layers are
//! placed on.

use aegle_platform::{
    KeyboardInteractivity, Layer, LayerOptions, OutputId, OutputInfo, PixelSize, Point,
    PopupOptions, Side,
};
use smithay_client_toolkit::{
    output::{OutputHandler, OutputState},
    shell::{
        WaylandSurface,
        wlr_layer::{self, LayerSurface},
        xdg::{
            XdgPositioner, XdgSurface,
            popup::{Popup, PopupConfigure, PopupHandler},
        },
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    protocol::{wl_output, wl_surface::WlSurface},
};
use wayland_protocols::xdg::shell::client::xdg_positioner::{
    Anchor as Edge, ConstraintAdjustment, Gravity,
};

use crate::{Error, Event, State, WindowId, state::Shell};

/// Creates a layer surface on `surface` with `options`, sized `size` along
/// every axis it does not stretch across.
pub(crate) fn layer(
    state: &State,
    qh: &QueueHandle<State>,
    surface: WlSurface,
    app_id: &str,
    size: PixelSize,
    options: &LayerOptions,
) -> Result<Shell, Error> {
    let Some(shell) = state.layer_shell.as_ref() else {
        surface.destroy();
        return Err(Error::Unsupported("zwlr_layer_shell_v1"));
    };
    let output = match options.output {
        Some(id) => match state.output(id) {
            Some(output) => Some(output),
            None => {
                surface.destroy();
                return Err(Error::InvalidRole("the output is not connected"));
            }
        },
        None => None,
    };
    let native = shell.create_layer_surface(
        qh,
        surface,
        stacking(options.layer),
        Some(app_id),
        output.as_ref(),
    );
    place(&native, options, size);
    Ok(Shell::Layer(native))
}

/// Applies everything but the layer and output to a layer surface; the
/// changes take effect with its next commit.
fn place(native: &LayerSurface, options: &LayerOptions, size: PixelSize) {
    let anchor = anchor(options.anchor);
    // Zero leaves a stretched axis to the compositor.
    let stretch = |edges: wlr_layer::Anchor, extent: u32| {
        if anchor.contains(edges) { 0 } else { extent }
    };
    native.set_size(
        stretch(
            wlr_layer::Anchor::LEFT | wlr_layer::Anchor::RIGHT,
            size.width,
        ),
        stretch(
            wlr_layer::Anchor::TOP | wlr_layer::Anchor::BOTTOM,
            size.height,
        ),
    );
    native.set_anchor(anchor);
    native.set_exclusive_zone(options.exclusive_zone);
    let [top, right, bottom, left] = options.margin;
    native.set_margin(top, right, bottom, left);
    native.set_keyboard_interactivity(match options.keyboard {
        KeyboardInteractivity::None => wlr_layer::KeyboardInteractivity::None,
        KeyboardInteractivity::Exclusive => wlr_layer::KeyboardInteractivity::Exclusive,
        KeyboardInteractivity::OnDemand => wlr_layer::KeyboardInteractivity::OnDemand,
    });
}

fn stacking(layer: Layer) -> wlr_layer::Layer {
    match layer {
        Layer::Background => wlr_layer::Layer::Background,
        Layer::Bottom => wlr_layer::Layer::Bottom,
        Layer::Top => wlr_layer::Layer::Top,
        Layer::Overlay => wlr_layer::Layer::Overlay,
    }
}

fn anchor(edges: aegle_platform::Anchor) -> wlr_layer::Anchor {
    use aegle_platform::Anchor;
    let mut anchor = wlr_layer::Anchor::empty();
    for (edge, native) in [
        (Anchor::TOP, wlr_layer::Anchor::TOP),
        (Anchor::BOTTOM, wlr_layer::Anchor::BOTTOM),
        (Anchor::LEFT, wlr_layer::Anchor::LEFT),
        (Anchor::RIGHT, wlr_layer::Anchor::RIGHT),
    ] {
        if edges.contains(edge) {
            anchor |= native;
        }
    }
    anchor
}

/// Creates an xdg popup on `surface` attached to its parent window, grabbing
/// the requested seat's latest press.
pub(crate) fn popup(
    state: &State,
    qh: &QueueHandle<State>,
    surface: WlSurface,
    size: PixelSize,
    options: &PopupOptions,
) -> Result<Shell, Error> {
    let fail = |surface: WlSurface, error| {
        surface.destroy();
        Err(error)
    };
    let Some(parent) = state.windows.iter().find(|w| w.id == options.parent) else {
        return fail(surface, Error::InvalidWindow);
    };
    let rect = options.anchor;
    let (x, y) = (rect.origin.x.round(), rect.origin.y.round());
    let (width, height) = (rect.size.width.round(), rect.size.height.round());
    let representable = |v: f32| v.is_finite() && v.abs() <= (i32::MAX / 4) as f32;
    if !(representable(x) && representable(y) && representable(width) && representable(height))
        || width < 1.0
        || height < 1.0
    {
        return fail(
            surface,
            Error::InvalidRole("a popup anchor must be nonempty"),
        );
    }
    let positioner = match XdgPositioner::new(&state.shell) {
        Ok(positioner) => positioner,
        Err(error) => return fail(surface, Error::backend(error)),
    };
    positioner.set_size(size.width as i32, size.height as i32);
    positioner.set_anchor_rect(x as i32, y as i32, width as i32, height as i32);
    let (edge, gravity) = match options.side {
        Side::Below => (Edge::BottomLeft, Gravity::BottomRight),
        Side::Above => (Edge::TopLeft, Gravity::TopRight),
        Side::Left => (Edge::TopLeft, Gravity::BottomLeft),
        Side::Right => (Edge::TopRight, Gravity::BottomRight),
    };
    positioner.set_anchor(edge);
    positioner.set_gravity(gravity);
    positioner.set_constraint_adjustment(
        ConstraintAdjustment::FlipX
            | ConstraintAdjustment::FlipY
            | ConstraintAdjustment::SlideX
            | ConstraintAdjustment::SlideY,
    );
    let xdg_parent = match &parent.window {
        Shell::Xdg(window) => Some(window.xdg_surface().clone()),
        Shell::Popup(popup) => Some(popup.xdg_surface().clone()),
        Shell::Layer(_) => None,
    };
    let native = match Popup::from_surface(
        xdg_parent.as_ref(),
        &positioner,
        qh,
        surface.clone(),
        &state.shell,
    ) {
        Ok(native) => native,
        Err(error) => return fail(surface, Error::backend(error)),
    };
    // A layer parent adopts the popup before its first commit.
    if let Shell::Layer(layer) = &parent.window {
        layer.get_popup(native.xdg_popup());
    }
    if let Some(seat) = options.grab
        && let Some((seat, serial)) = state.latest_press(seat)
    {
        native.xdg_popup().grab(&seat, serial);
    }
    Ok(Shell::Popup(native))
}

impl crate::Wayland {
    /// Replaces a layer surface's placement and preferred size in place, then
    /// commits it so the compositor configures the new geometry.
    pub(crate) fn set_layer(
        &mut self,
        id: WindowId,
        options: LayerOptions,
        size: PixelSize,
    ) -> Result<(), Error> {
        if size.width == 0 || size.height == 0 {
            return Err(Error::InvalidSize);
        }
        let window = self
            .state
            .windows
            .iter_mut()
            .find(|w| w.id == id)
            .ok_or(Error::InvalidWindow)?;
        let (Shell::Layer(native), Some(current)) = (&window.window, &mut window.layer) else {
            return Err(Error::InvalidRole("the window is not a layer surface"));
        };
        if current.output != options.output {
            return Err(Error::InvalidRole(
                "a layer surface cannot change its output",
            ));
        }
        if current.layer != options.layer {
            // `SurfaceKind` is non-exhaustive; only wlr surfaces exist.
            let wlr_layer::SurfaceKind::Wlr(wlr) = native.kind() else {
                unreachable!("layer surfaces are wlr surfaces")
            };
            if wlr.version() < 2 {
                return Err(Error::Unsupported("changing the layer of a layer surface"));
            }
            native.set_layer(stacking(options.layer));
        }
        place(native, &options, size);
        *current = options;
        native.commit();
        Ok(())
    }

    /// The outputs whose geometry the compositor has described.
    pub(crate) fn outputs(&self) -> Vec<OutputInfo> {
        let outputs = &self.state.output_state;
        outputs
            .outputs()
            .filter_map(|output| outputs.info(&output))
            .map(|info| {
                let mode = info.modes.iter().find(|mode| mode.current);
                let integer = info.scale_factor.max(1);
                let (width, height) = info.logical_size.unwrap_or_else(|| {
                    mode.map_or((0, 0), |m| {
                        (m.dimensions.0 / integer, m.dimensions.1 / integer)
                    })
                });
                // The current mode over the logical extent gives a fractional
                // scale; a rotated output swaps the mode's axes.
                let rotated = matches!(
                    info.transform,
                    wl_output::Transform::_90
                        | wl_output::Transform::_270
                        | wl_output::Transform::Flipped90
                        | wl_output::Transform::Flipped270
                );
                let scale = match mode {
                    Some(mode) if width > 0 => {
                        let physical = if rotated {
                            mode.dimensions.1
                        } else {
                            mode.dimensions.0
                        };
                        physical as f32 / width as f32
                    }
                    _ => integer as f32,
                };
                let (x, y) = info.logical_position.unwrap_or(info.location);
                let description = info.description.clone().unwrap_or_else(|| {
                    [info.make.as_str(), info.model.as_str()]
                        .iter()
                        .filter(|part| !part.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" ")
                });
                OutputInfo {
                    id: OutputId(info.id.into()),
                    name: info.name.clone().unwrap_or_default(),
                    description,
                    position: Point::new(x as f32, y as f32),
                    size: PixelSize {
                        width: width.max(0) as u32,
                        height: height.max(0) as u32,
                    },
                    scale,
                }
            })
            .collect()
    }
}

impl State {
    /// The connected output with this identity.
    fn output(&self, id: OutputId) -> Option<wl_output::WlOutput> {
        self.output_state.outputs().find(|output| {
            self.output_state
                .info(output)
                .is_some_and(|info| u64::from(info.id) == id.0)
        })
    }

    /// Queues one [`Event::Outputs`] for any number of pending changes.
    fn outputs_changed(&mut self) {
        if !self
            .events
            .iter()
            .any(|event| matches!(event, Event::Outputs))
        {
            self.events.push_back(Event::Outputs);
        }
    }
}

impl PopupHandler for State {
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        popup: &Popup,
        configure: PopupConfigure,
    ) {
        let extent = |value: i32| (value > 0).then_some(value as u32);
        let size = (extent(configure.width), extent(configure.height));
        self.configured(popup.wl_surface(), size, true);
    }

    fn done(&mut self, _: &Connection, _: &QueueHandle<Self>, popup: &Popup) {
        if let Some(window) = self.window_id(popup.wl_surface()) {
            self.events.push_back(Event::Close { window });
        }
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.outputs_changed();
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.outputs_changed();
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.outputs_changed();
    }
}
