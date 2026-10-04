//! Explicitly owned timers; SCTK retains responsibility for ordinary key/compose events.

use std::time::{Duration, Instant};

use smithay_client_toolkit::{
    reexports::calloop::{
        LoopHandle, RegistrationToken,
        timer::{TimeoutAction, Timer},
    },
    seat::keyboard::{KeyEvent, Keymap, RawModifiers, RepeatInfo},
};
use wayland_client::protocol::wl_keyboard::WlKeyboard;
use xkbcommon::xkb;

use crate::{Error, Event, State};

pub(super) struct Repeat {
    // SCTK does not expose its keymap. This second state is necessary for
    // repeatability and modifier/group-aware translation of retained repeats.
    xkb: Option<xkb::State>,
    info: RepeatInfo,
    key: Option<KeyEvent>,
    token: Option<RegistrationToken>,
    // Owned by State, never captured by a registered callback. A callback only
    // holds the plain SCTK keyboard, whose data contains no loop handle.
    handle: LoopHandle<'static, State>,
}

impl Repeat {
    pub(super) fn new(handle: &LoopHandle<'static, State>) -> Self {
        Self {
            xkb: None,
            info: RepeatInfo::Disable,
            key: None,
            token: None,
            handle: handle.clone(),
        }
    }

    pub(super) fn keymap(&mut self, keymap: Keymap<'_>) -> Result<(), Error> {
        self.cancel();
        self.xkb = None;
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let map = xkb::Keymap::new_from_string(
            &context,
            keymap.as_string(),
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::COMPILE_NO_FLAGS,
        )
        .ok_or_else(|| Error::backend("cannot compile keyboard repeat keymap"))?;
        self.xkb = Some(xkb::State::new(&map));
        Ok(())
    }

    pub(super) fn modifiers(&mut self, raw: RawModifiers, group: u32) {
        if let Some(state) = &mut self.xkb {
            state.update_mask(raw.depressed, raw.latched, raw.locked, 0, 0, group);
            if let Some(key) = &mut self.key {
                let code = xkb::Keycode::new(key.raw_code + 8);
                key.keysym = state.key_get_one_sym(code);
                // Preserve a pending compose sequence's absence of text. Once
                // there is committed key text, modifier changes retranslate it.
                if key.utf8.is_some() {
                    key.utf8 = Some(state.key_get_utf8(code));
                }
            }
        }
    }

    pub(super) fn configure(
        &mut self,
        info: RepeatInfo,
        keyboard: &WlKeyboard,
    ) -> Result<(), Error> {
        self.stop_timer();
        self.info = info;
        self.arm(keyboard)
    }

    pub(super) fn press(&mut self, key: &KeyEvent, keyboard: &WlKeyboard) -> Result<(), Error> {
        if self.xkb.as_ref().is_some_and(|state| {
            state
                .get_keymap()
                .key_repeats(xkb::Keycode::new(key.raw_code + 8))
        }) {
            self.cancel();
            self.key = Some(key.clone());
            self.arm(keyboard)?;
        }
        Ok(())
    }

    pub(super) fn release(&mut self, raw_code: u32) {
        if self
            .key
            .as_ref()
            .is_some_and(|key| key.raw_code == raw_code)
        {
            self.cancel();
        }
    }

    /// Compositor-generated repeats already have a timestamp and never need
    /// a client timer. Preserve composed text from the initial press.
    pub(super) fn server_event(&self, mut event: KeyEvent) -> KeyEvent {
        if let Some(key) = &self.key {
            if key.raw_code == event.raw_code {
                event.keysym = key.keysym;
                event.utf8.clone_from(&key.utf8);
                return event;
            }
        }
        if let Some(state) = &self.xkb {
            let code = xkb::Keycode::new(event.raw_code + 8);
            event.keysym = state.key_get_one_sym(code);
            event.utf8 = Some(state.key_get_utf8(code));
        }
        event
    }

    pub(super) fn cancel(&mut self) {
        self.stop_timer();
        self.key = None;
    }

    pub(super) fn remove_keyboard(&mut self) {
        self.cancel();
        self.xkb = None;
        self.info = RepeatInfo::Disable;
    }

    fn stop_timer(&mut self) {
        if let Some(token) = self.token.take() {
            self.handle.remove(token);
        }
    }

    fn arm(&mut self, keyboard: &WlKeyboard) -> Result<(), Error> {
        let (Some(key), RepeatInfo::Repeat { delay, .. }) = (&self.key, self.info) else {
            return Ok(());
        };
        let keyboard = keyboard.clone();
        let base_time = key.time;
        let start = Instant::now();
        self.token = Some(
            self.handle
                .insert_source(
                    Timer::from_duration(Duration::from_millis(u64::from(delay))),
                    move |_, _, state| {
                        // Wayland millisecond timestamps wrap. Measure actual elapsed
                        // time so high repeat rates do not accumulate rounding error.
                        let time = base_time.wrapping_add(start.elapsed().as_millis() as u32);
                        state.repeat_tick(&keyboard, time)
                    },
                )
                .map_err(Error::backend)?,
        );
        Ok(())
    }
}

impl Drop for Repeat {
    fn drop(&mut self) {
        self.stop_timer();
    }
}

impl State {
    fn repeat_tick(&mut self, keyboard: &WlKeyboard, time: u32) -> TimeoutAction {
        let Some(input) = self
            .input
            .seats
            .iter_mut()
            .find(|input| input.keyboard.as_ref() == Some(keyboard))
        else {
            return TimeoutAction::Drop;
        };
        let (Some((window, _)), Some(key), RepeatInfo::Repeat { rate, .. }) =
            (&input.focus, &mut input.repeat.key, input.repeat.info)
        else {
            input.repeat.token = None;
            return TimeoutAction::Drop;
        };
        key.time = time;
        let key = key.clone();
        self.events.push_back(Event::Key {
            window: *window,
            seat: input.seat.clone(),
            key,
            pressed: true,
            repeat: true,
            modifiers: input.modifiers,
        });
        TimeoutAction::ToDuration(Duration::from_nanos(
            (1_000_000_000 / u64::from(rate.get())).max(1),
        ))
    }
}
