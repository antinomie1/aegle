use crate::native::{App, Runtime};
use crate::platform::Event;
use aegle_ui::{Result, Theme, Ui, UiError};
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
            if matches!(event, Event::Wake) {
                self.drain_proxies()?;
            } else {
                self.runtime.borrow_mut().event(event)?;
            }
            self.callbacks(callbacks)?;
            // A focus switch cancels the old protocol session before the next
            // queued event, preventing an old IME batch from editing a new field.
            self.runtime.borrow_mut().refresh()?;
        }
        #[cfg(any(
            all(feature = "unix-accessibility", target_os = "linux"),
            all(feature = "windows-accessibility", target_os = "windows")
        ))]
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
    /// The window theme resolved from options and system preferences.
    pub fn theme(&self) -> Theme {
        let (options, preferences) = (&self.options, self.preferences);
        let pick =
            |wanted: Option<bool>, theme: Option<Theme>| theme.filter(|_| wanted == Some(true));
        let mut theme = pick(preferences.high_contrast, options.high_contrast_theme)
            .or(pick(preferences.dark, options.dark_theme))
            .unwrap_or(options.theme);
        if let Some(percent) = options.text_scale.or(preferences.text_scale) {
            let scale = f32::from(percent.clamp(50, 400)) / 100.0;
            theme.font_size *= scale;
            theme.control_height *= scale;
        }
        theme
    }

    #[cfg(feature = "motion")]
    pub fn reduced_motion(&self) -> bool {
        self.options
            .reduced_motion
            .or(self.preferences.reduced_motion)
            .unwrap_or(false)
    }

    /// Re-resolves windows that still use the previously resolved values.
    fn preferences(&mut self, preferences: crate::platform::Preferences) -> Result<()> {
        let theme = self.theme();
        #[cfg(feature = "motion")]
        let reduced = self.reduced_motion();
        self.preferences = preferences;
        for entry in &self.windows {
            let current = entry.ui.theme();
            if current == theme {
                entry.ui.set_theme(self.theme())?;
            }
            #[cfg(feature = "motion")]
            if entry.ui.reduced_motion() == reduced {
                entry.ui.set_reduced_motion(self.reduced_motion())?;
            }
        }
        Ok(())
    }

    fn event(&mut self, event: Event) -> Result<()> {
        let event = match event {
            Event::Error(error) => return Err(error.into()),
            Event::Preferences(preferences) => return self.preferences(preferences),
            event => event,
        };
        let Some(id) = crate::native_input::target(&event) else {
            return Ok(());
        };
        // Window removal prunes queued platform events; unknown IDs can only
        // arise when another callback closed this window during this dispatch.
        let Some(index) = self.windows.iter().position(|entry| entry.id == id) else {
            return Ok(());
        };
        if matches!(event, Event::Close { .. }) {
            self.close(id)?;
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
            // Synchronous pastes land before this refresh records them.
            if let Some(request) = entry.ui.take_clipboard()? {
                crate::native_input::clipboard(&mut self.backend, entry, request)?;
            }
            if entry.ui.refresh()? {
                self.backend.request_redraw(entry.id)?;
            }
            // Layout, visibility and focus can change the control under a
            // stationary pointer, so this runs after every refresh, not only
            // after pointer events.
            let cursor = entry.ui.cursor()?;
            if cursor != entry.cursor {
                self.backend.set_cursor(entry.id, cursor)?;
                entry.cursor = cursor;
            }
            if let Some(ime) = entry.ui.take_ime_state(4000)? {
                if ime.reset && self.backend.ime_available() {
                    self.backend.configure_ime(entry.id, None)?;
                }
                let request = crate::native_input::ime_request(ime.request);
                // Missing IME is a capability error when requested, never an
                // apparently successful keyboard-only editable control.
                if request.is_some() || self.backend.ime_available() {
                    self.backend.configure_ime(entry.id, request)?;
                }
            }
            #[cfg(any(
                all(feature = "unix-accessibility", target_os = "linux"),
                all(feature = "windows-accessibility", target_os = "windows")
            ))]
            if entry.initial_access || entry.ui.access_dirty() {
                #[cfg(target_os = "linux")]
                let scale = 1.0;
                #[cfg(target_os = "windows")]
                let scale = entry.access_scale;
                entry
                    .ui
                    .publish_accessibility(&entry.title, scale, |build| {
                        entry.accessibility.update_if_active(build);
                    })?;
                entry.initial_access = false;
            }
        }
        Ok(())
    }
}
