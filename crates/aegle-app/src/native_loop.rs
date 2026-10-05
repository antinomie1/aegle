use crate::native::{App, Runtime};
use crate::{Result, Ui, UiError};
use aegle_platform_wayland::{Event, ImeCause, ImeHints, ImeRequest};
use aegle_render_software::Surface;
use aegle_scene::Affine;
use std::{cell::Cell, rc::Rc, time::Duration};

struct DispatchGuard<'a>(&'a Cell<bool>);
impl Drop for DispatchGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl App {
    pub(crate) fn run_loop(self) -> Result<()> {
        let mut callbacks = Vec::new();
        while self.pump(None, &mut callbacks)? {}
        Ok(())
    }

    /// Processes queued changes, waits at most `timeout`, then drains native
    /// events and presents ready windows. `None` blocks until work arrives.
    /// Returns false after the last window closes. Use [`Self::run`] for the
    /// ordinary standalone event loop, which reuses its callback scratch vector.
    pub fn dispatch(&self, timeout: Option<Duration>) -> Result<bool> {
        self.pump(timeout, &mut Vec::new())
    }

    fn pump(&self, timeout: Option<Duration>, callbacks: &mut Vec<Rc<Ui>>) -> Result<bool> {
        if self.dispatching.replace(true) {
            return Err(UiError::ReentrantAccess.into());
        }
        let _dispatch = DispatchGuard(&self.dispatching);
        self.callbacks(callbacks)?;
        self.runtime.borrow_mut().refresh()?;
        if self.runtime.borrow().windows.is_empty() {
            return Ok(false);
        }
        let pending = self
            .runtime
            .borrow()
            .windows
            .iter()
            .any(|entry| entry.ui.has_pending_callbacks());
        self.runtime.borrow_mut().backend.dispatch(if pending {
            Some(Duration::ZERO)
        } else {
            timeout
        })?;
        loop {
            let event = self.runtime.borrow_mut().backend.next_event();
            let Some(event) = event else { break };
            self.runtime.borrow_mut().event(event)?;
            self.callbacks(callbacks)?;
            // A focus switch cancels the old protocol session before the next
            // queued event, preventing an old IME batch from editing a new field.
            self.runtime.borrow_mut().refresh()?;
        }
        #[cfg(feature = "unix-accessibility")]
        loop {
            let pending = {
                let mut runtime = self.runtime.borrow_mut();
                runtime.windows.iter_mut().find_map(|entry| {
                    entry
                        .accessibility
                        .next_event()
                        .map(|event| (entry.id, event))
                })
            };
            let Some((id, event)) = pending else { break };
            {
                let mut runtime = self.runtime.borrow_mut();
                let entry = runtime
                    .windows
                    .iter_mut()
                    .find(|entry| entry.id == id)
                    .unwrap();
                match event {
                    aegle_access::Event::InitialTree => entry.initial_access = true,
                    aegle_access::Event::Action(action) => {
                        entry.ui.access_action(action)?;
                    }
                    aegle_access::Event::Deactivate => {}
                }
            }
            self.callbacks(callbacks)?;
            self.runtime.borrow_mut().refresh()?;
        }
        self.runtime.borrow_mut().present()?;
        Ok(!self.runtime.borrow().windows.is_empty())
    }

    fn callbacks(&self, scratch: &mut Vec<Rc<Ui>>) -> Result<()> {
        scratch.clear();
        scratch.extend(
            self.runtime
                .borrow()
                .windows
                .iter()
                .map(|entry| entry.ui.clone()),
        );
        for ui in scratch.drain(..) {
            ui.dispatch_callbacks()?;
        }
        Ok(())
    }
}

impl Runtime {
    fn event(&mut self, event: Event) -> Result<()> {
        if let Event::Error(error) = event {
            return Err(error.into());
        }
        let Some(id) = crate::native_input::target(&event) else {
            return Ok(());
        };
        // Window removal prunes queued platform events; unknown IDs can only
        // arise when another callback closed this window during this dispatch.
        let Some(index) = self.windows.iter().position(|entry| entry.id == id) else {
            return Ok(());
        };
        if matches!(event, Event::Close { .. }) {
            self.backend.remove_window(id)?;
            self.windows[index].ui.close()?;
            self.windows.remove(index);
        } else {
            self.windows[index].event(event)?;
        }
        Ok(())
    }

    fn refresh(&mut self) -> Result<()> {
        #[cfg(feature = "motion")]
        let now = self.clock.elapsed();
        for entry in &mut self.windows {
            #[cfg(feature = "motion")]
            entry.ui.advance_animations(now)?;
            if entry.ui.refresh()? {
                self.backend.request_redraw(entry.id)?;
            }
            if let Some(ime) = entry.ui.take_ime_state(4000)? {
                if ime.reset && self.backend.ime_available() {
                    self.backend.configure_ime(entry.id, None)?;
                }
                let request = ime.request.map(|request| ImeRequest {
                    surrounding: request.surrounding,
                    cursor: request.selection.focus,
                    anchor: request.selection.anchor,
                    cursor_rect: request.cursor_rect,
                    hints: if request.multiline {
                        ImeHints::Multiline
                    } else {
                        ImeHints::empty()
                    },
                    cause: if request.input_method {
                        ImeCause::InputMethod
                    } else {
                        ImeCause::Other
                    },
                    ..Default::default()
                });
                // Missing IME is a capability error when requested, never an
                // apparently successful keyboard-only editable control.
                if request.is_some() || self.backend.ime_available() {
                    self.backend.configure_ime(entry.id, request)?;
                }
            }
            #[cfg(feature = "unix-accessibility")]
            if entry.initial_access || entry.ui.access_dirty() {
                entry.ui.publish_accessibility(&entry.title, |build| {
                    entry.accessibility.update_if_active(build);
                })?;
                entry.initial_access = false;
            }
        }
        Ok(())
    }

    fn present(&mut self) -> Result<()> {
        for entry in &mut self.windows {
            if !std::mem::take(&mut entry.ready) {
                continue;
            }
            let info = self.backend.window_info(entry.id)?;
            let scale = Affine::scale(info.scale as f32, info.scale as f32)?;
            let background = entry.ui.background();
            let renderer = &mut self.renderer;
            self.backend
                .present(entry.id, |pixels, size| {
                    let mut surface = Surface::new(pixels, size.width, size.height)?;
                    let mut frame = renderer.begin_frame(&mut surface, background);
                    entry.ui.visit_scenes(|scene, transform, clip| {
                        let clip = clip.map(|rect| {
                            let factor = info.scale as f32;
                            aegle_types::Rect::new(
                                rect.origin.x * factor,
                                rect.origin.y * factor,
                                rect.size.width * factor,
                                rect.size.height * factor,
                            )
                        });
                        frame.draw_clipped(scene, transform.then(scale)?, clip)?;
                        Ok(())
                    })
                })
                .map_err(|error| format!("present: {error}"))?;
            // The backend waits for a frame callback and a free buffer. An
            // occluded window therefore adds no animation timer or idle poll.
            #[cfg(feature = "motion")]
            if entry.ui.has_animations() {
                self.backend.request_redraw(entry.id)?;
            }
        }
        Ok(())
    }
}
