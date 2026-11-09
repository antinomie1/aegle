//! Desktop-shell surfaces: outputs, live layer placement and native popups.

use crate::native::{App, Window, WindowOptions};
use aegle_platform::{LayerOptions, OutputInfo, PixelSize, Platform, Side};
use aegle_ui::{Rect, Result, UiError};

/// See [`App::on_outputs`].
pub(crate) type OutputsHandler = Box<dyn FnMut(&App) -> Result<()>>;

/// A native popup's placement against its parent window, its size and focus.
///
/// Popups are surfaces of their own, so unlike an in-window `Popup` control
/// they can extend past a small parent such as a panel.
#[derive(Clone, Copy, Debug)]
pub struct PopupOptions {
    /// The rectangle of the parent's content the popup attaches to, in the
    /// parent's logical coordinates, such as a control's `bounds()`; its
    /// extent must be at least one dp.
    pub anchor: Rect,
    /// The side of `anchor` it opens on; the window system flips or slides
    /// it to keep it on screen.
    pub side: Side,
    /// Takes the keyboard and closes on a press outside its popup chain, as
    /// a menu does; without it the popup is passive, like a tooltip. A grab
    /// cites the parent's latest press, so open it from a press handler.
    pub grab: bool,
    /// Preferred logical width.
    pub width: u32,
    /// Preferred logical height.
    pub height: u32,
    /// Maximum software presentation bytes, as for [`WindowOptions`].
    pub buffer_budget: usize,
}

impl PopupOptions {
    /// A grabbing popup of `width` × `height` below `anchor`.
    pub fn new(anchor: Rect, width: u32, height: u32) -> Self {
        Self {
            anchor,
            side: Side::Below,
            grab: true,
            width,
            height,
            buffer_budget: usize::MAX,
        }
    }
}

impl App {
    /// The connected display outputs, for placing layers with
    /// [`LayerOptions::output`]. Fails with an unsupported-capability error
    /// on platforms that cannot enumerate them (Win32 for now).
    pub fn outputs(&self) -> Result<Vec<OutputInfo>> {
        Ok(self.runtime.borrow().backend.outputs()?)
    }

    /// The connected output the window system calls `name`, such as `DP-1`.
    pub fn output(&self, name: &str) -> Result<OutputInfo> {
        self.outputs()?
            .into_iter()
            .find(|output| output.name == name)
            .ok_or_else(|| format!("no connected output is named `{name}`").into())
    }

    /// Runs `handler` after outputs connect, disconnect or change geometry,
    /// once per dispatch however many changed, outside every runtime borrow:
    /// it may open or close windows, for example one panel per output. An
    /// error goes to [`Self::on_error`]. Replaces the previous handler.
    pub fn on_outputs(&self, handler: impl FnMut(&App) -> Result<()> + 'static) {
        *self.on_outputs.borrow_mut() = Some(Box::new(handler));
    }

    /// Runs the outputs handler, which may replace itself meanwhile.
    pub(crate) fn outputs_changed(&self) -> Result<()> {
        let Some(mut handler) = self.on_outputs.borrow_mut().take() else {
            return Ok(());
        };
        let result = handler(self);
        self.on_outputs.borrow_mut().get_or_insert(handler);
        self.report(result)
    }
}

impl Window {
    /// Changes this layer's placement and preferred logical size in place,
    /// keeping its native surface, controls and state; the new geometry
    /// arrives as a resize. The output cannot change, and a window that is
    /// not a layer fails.
    pub fn set_layer(&self, layer: LayerOptions, width: u32, height: u32) -> Result<()> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let mut runtime = runtime.borrow_mut();
        let size = PixelSize { width, height };
        runtime.backend.set_layer(self.id, layer, size)?;
        Ok(())
    }

    /// Opens a native popup attached to this window, with its own control
    /// tree; it closes with this window, and closes itself (removing its
    /// controls) when the window system dismisses it. Platforms without
    /// popups fail with an unsupported-capability error (Win32 for now); SDL3
    /// slides a popup onto the screen but never flips it.
    pub fn popup(&self, options: PopupOptions) -> Result<Window> {
        let runtime = self.runtime.upgrade().ok_or(UiError::DeadHandle)?;
        let grab = match options.grab {
            false => None,
            true => {
                let runtime = runtime.borrow();
                let entry = runtime.windows.iter().find(|e| e.id == self.id);
                // A window no seat has used yet has no press to cite.
                entry.ok_or(UiError::DeadHandle)?.seat
            }
        };
        let window = WindowOptions {
            width: options.width,
            height: options.height,
            buffer_budget: options.buffer_budget,
            ..WindowOptions::default()
        };
        let popup = aegle_platform::PopupOptions {
            parent: self.id,
            anchor: options.anchor,
            side: options.side,
            grab,
        };
        crate::native::open(&runtime, "", window, Some(popup))
    }
}
