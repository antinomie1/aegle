//! A window's TSF text store: the host's surrounding excerpt with the
//! composition inline, behind `ITextStoreACP`.
//!
//! The host stays authoritative. Input-method edits are reported after each
//! granted lock as one [`ImeUpdate`](crate::ImeUpdate) relative to what the
//! host shows; whenever the host publishes outside a composition, its excerpt
//! and selection replace the document and TSF is told.
#![allow(unsafe_code)]

use crate::{
    Error, Event, ImeEvent,
    ime_edit::{self, Document, Shown},
    native::Native,
};
use std::{
    cell::{Ref as Borrow, RefCell, RefMut},
    ops::Range,
    rc::{Rc, Weak},
};
use windows::{
    Win32::{
        Foundation::{E_INVALIDARG, HWND, RECT},
        UI::TextServices::*,
    },
    core::{IUnknown, Interface, Result, implement},
};

mod acp;

/// The store's only view.
const VIEW: u32 = 1;
/// Largest document an input method may build, in UTF-16 units.
const MAX_UNITS: usize = 512 * 1024;

struct Sink {
    sink: ITextStoreACPSink,
    identity: IUnknown,
    mask: u32,
}

#[derive(Default)]
struct State {
    sink: Option<Sink>,
    /// The granted lock, `TS_LF_READ` or `TS_LF_READWRITE`; 0 when unlocked.
    lock: u32,
    /// An asynchronous request granted when the current lock ends.
    queued: u32,
    text: Vec<u16>,
    /// Anchor and active end.
    selection: (usize, usize),
    composition: Option<ITfCompositionView>,
    /// What the host displays; edits are reported relative to it.
    shown: Shown,
    /// Edits reach the host only while its editor session is enabled.
    reporting: bool,
    /// Caret rectangle in physical client pixels, once published.
    caret: Option<RECT>,
}

#[implement(ITextStoreACP, ITfContextOwnerCompositionSink)]
pub(crate) struct TextStore {
    native: Weak<Native>,
    state: RefCell<State>,
}

fn invalid_position() -> windows::core::Error {
    TS_E_INVALIDPOS.into()
}

/// Whether `index` falls between the units of a surrogate pair.
fn splits(text: &[u16], index: usize) -> bool {
    text.get(index)
        .is_some_and(|unit| (0xdc00..0xe000).contains(unit))
}

/// A validated range; `end == -1` means the document end.
fn range(text: &[u16], start: i32, end: i32) -> Result<Range<usize>> {
    let end = if end == -1 { text.len() as i32 } else { end };
    if start < 0 || start > end || end as usize > text.len() {
        return Err(invalid_position());
    }
    let (start, end) = (start as usize, end as usize);
    if splits(text, start) || splits(text, end) {
        return Err(invalid_position());
    }
    Ok(start..end)
}

/// The common-prefix and common-suffix change between two documents.
fn change(old: &[u16], new: &[u16]) -> TS_TEXTCHANGE {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    TS_TEXTCHANGE {
        acpStart: prefix as i32,
        acpOldEnd: (old.len() - suffix) as i32,
        acpNewEnd: (new.len() - suffix) as i32,
    }
}

impl TextStore {
    pub fn new(native: Weak<Native>) -> Self {
        Self {
            native,
            state: RefCell::default(),
        }
    }

    fn native(&self) -> Option<Rc<Native>> {
        self.native.upgrade().filter(|n| n.registered.get())
    }

    fn hwnd(&self) -> HWND {
        self.native
            .upgrade()
            .map_or(HWND::default(), |n| n.hwnd.get())
    }

    fn read(&self) -> Result<Borrow<'_, State>> {
        let state = self.state.borrow();
        if state.lock & TS_LF_READ.0 == 0 {
            return Err(TS_E_NOLOCK.into());
        }
        Ok(state)
    }

    fn write(&self) -> Result<RefMut<'_, State>> {
        let state = self.state.borrow_mut();
        if state.lock & TS_LF_READWRITE.0 != TS_LF_READWRITE.0 {
            return Err(TS_E_NOLOCK.into());
        }
        Ok(state)
    }

    /// Whether an input method is composing in this document.
    pub fn composing(&self) -> bool {
        self.state.borrow().composition.is_some()
    }

    /// Starts reporting input-method edits to the host.
    pub fn begin(&self) {
        self.state.borrow_mut().reporting = true;
    }

    /// Stops reporting before a composition is terminated; the host cancels
    /// its own preedit, and its next publish replaces the document.
    pub fn stop(&self) {
        self.state.borrow_mut().reporting = false;
    }

    /// Forgets a composition after TSF terminated it.
    pub fn forget_composition(&self) {
        self.state.borrow_mut().composition = None;
    }

    /// Mirrors the host's editor. During a composition the input method owns
    /// the document and only the caret rectangle follows the host.
    pub fn publish(&self, surrounding: Option<&str>, selection: (usize, usize), caret: RECT) {
        let text: Vec<u16> = surrounding.unwrap_or("").encode_utf16().collect();
        let index = |byte: usize| surrounding.map_or(0, |s| s[..byte].encode_utf16().count());
        let selection = (index(selection.0), index(selection.1));
        let (sink, text_change, selection_change, layout) = {
            let mut state = self.state.borrow_mut();
            let layout = state.caret.replace(caret) != Some(caret);
            let (mut text_change, mut selection_change) = (None, false);
            if state.composition.is_none() {
                text_change = (state.text != text).then(|| change(&state.text, &text));
                selection_change = text_change.is_some() || state.selection != selection;
                let (low, high) = (selection.0.min(selection.1), selection.0.max(selection.1));
                state.shown = Shown {
                    text: text.clone(),
                    slot: low..high,
                    ..Shown::default()
                };
                state.text = text;
                state.selection = selection;
            }
            let Some(sink) = state.sink.as_ref().map(|s| (s.sink.clone(), s.mask)) else {
                return;
            };
            (sink, text_change, selection_change, layout)
        };
        let (sink, mask) = sink;
        // SAFETY: no lock is held and no state is borrowed; TSF may call back.
        unsafe {
            if let Some(change) = text_change
                && mask & TS_AS_TEXT_CHANGE != 0
            {
                let _ = sink.OnTextChange(TS_ST_NONE, &change);
            }
            if selection_change && mask & TS_AS_SEL_CHANGE != 0 {
                let _ = sink.OnSelectionChange();
            }
            if layout && mask & TS_AS_LAYOUT_CHANGE != 0 {
                let _ = sink.OnLayoutChange(TS_LC_CHANGE, VIEW);
            }
        }
    }

    /// The composition's validated extent; called while the lock is held so
    /// that TSF can read the store.
    fn composition(&self) -> std::result::Result<Option<Range<usize>>, Error> {
        let Some(view) = self.state.borrow().composition.clone() else {
            return Ok(None);
        };
        let (mut start, mut length) = (0, 0);
        // SAFETY: a live composition view; outputs are initialized integers.
        unsafe {
            view.GetRange()?
                .cast::<ITfRangeACP>()?
                .GetExtent(&mut start, &mut length)?;
        }
        let state = self.state.borrow();
        let end = start.checked_add(length).ok_or(invalid_position())?;
        let range = range(&state.text, start, end)
            .map_err(|_| Error::InvalidIme("composition outside the document"))?;
        Ok((!range.is_empty()).then_some(range))
    }

    /// Reports edits made under the lock that just ended.
    fn report(&self, composition: std::result::Result<Option<Range<usize>>, Error>) {
        let Some(native) = self.native() else { return };
        let mut state = self.state.borrow_mut();
        if !state.reporting {
            return;
        }
        let state = &mut *state;
        let result = match composition {
            Ok(composition) => {
                let document = Document {
                    text: &state.text,
                    composition: composition.clone(),
                    selection: state.selection,
                };
                ime_edit::edit(&state.shown, &document).map_err(|error| (error, composition))
            }
            Err(error) => Err((error, None)),
        };
        let event = match result {
            Ok(None) => return,
            Ok(Some((update, shown))) => {
                state.shown = shown;
                Event::Ime {
                    window: native.id,
                    event: ImeEvent::Update(update),
                }
            }
            Err((error, slot)) => {
                // The host keeps its state: report once rather than on every
                // later lock, until a publish after the composition replaces
                // the document.
                let (anchor, focus) = state.selection;
                state.shown = Shown {
                    text: state.text.clone(),
                    composing: slot.is_some(),
                    slot: slot.unwrap_or(anchor.min(focus)..anchor.max(focus)),
                    cursor: None,
                };
                Event::Error(error)
            }
        };
        native.emit(event);
    }

    fn replace(state: &mut State, range: Range<usize>, units: &[u16]) -> Result<TS_TEXTCHANGE> {
        if state.text.len() - range.len() + units.len() > MAX_UNITS {
            return Err(E_INVALIDARG.into());
        }
        let shift = |p: usize| {
            if p <= range.start {
                p
            } else if p >= range.end {
                p - range.len() + units.len()
            } else {
                range.start + units.len()
            }
        };
        state.selection = (shift(state.selection.0), shift(state.selection.1));
        state.text.splice(range.clone(), units.iter().copied());
        Ok(TS_TEXTCHANGE {
            acpStart: range.start as i32,
            acpOldEnd: range.end as i32,
            acpNewEnd: (range.start + units.len()) as i32,
        })
    }
}
