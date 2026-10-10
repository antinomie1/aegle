//! The COM interfaces of [`TextStore`]: `ITextStoreACP` for documents and
//! locks, and `ITfContextOwnerCompositionSink` for composition tracking.

use super::{MAX_UNITS, Sink, TextStore, TextStore_Impl, VIEW, range};
use std::mem;
use windows::{
    Win32::{
        Foundation::{E_INVALIDARG, E_NOTIMPL, E_UNEXPECTED, HWND, POINT, RECT, S_OK},
        Graphics::Gdi::ClientToScreen,
        System::{
            Com::{FORMATETC, IDataObject},
            Ole::{CONNECT_E_ADVISELIMIT, CONNECT_E_NOCONNECTION},
        },
        UI::{TextServices::*, WindowsAndMessaging::GetClientRect},
    },
    core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result},
};

/// Borrows `cch` units of well-formed UTF-16 from an input method.
fn units(text: &PCWSTR, cch: u32) -> Result<&[u16]> {
    if cch == 0 {
        return Ok(&[]);
    }
    if text.is_null() || cch as usize > MAX_UNITS {
        return Err(E_INVALIDARG.into());
    }
    // SAFETY: TSF passes `cch` readable units at a non-null pointer.
    let units = unsafe { std::slice::from_raw_parts(text.0, cch as usize) };
    if char::decode_utf16(units.iter().copied()).any(|c| c.is_err()) {
        return Err(E_INVALIDARG.into());
    }
    Ok(units)
}

/// Writes `value` through an optional output pointer.
fn put<T>(pointer: *mut T, value: T) {
    if !pointer.is_null() {
        // SAFETY: TSF output pointers are writable when non-null.
        unsafe { pointer.write(value) };
    }
}

impl ITextStoreACP_Impl for TextStore_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<IUnknown>, mask: u32) -> Result<()> {
        // SAFETY: TSF passes a valid interface identifier.
        if unsafe { *riid } != ITextStoreACPSink::IID {
            return Err(E_INVALIDARG.into());
        }
        let identity: IUnknown = punk.ok()?.cast()?;
        let mut state = self.state.borrow_mut();
        match &mut state.sink {
            Some(sink) if sink.identity == identity => sink.mask = mask,
            Some(_) => return Err(CONNECT_E_ADVISELIMIT.into()),
            None => {
                state.sink = Some(Sink {
                    sink: identity.cast()?,
                    identity,
                    mask,
                })
            }
        }
        Ok(())
    }

    fn UnadviseSink(&self, punk: Ref<IUnknown>) -> Result<()> {
        let identity: IUnknown = punk.ok()?.cast()?;
        let mut state = self.state.borrow_mut();
        if state.sink.as_ref().is_none_or(|s| s.identity != identity) {
            return Err(CONNECT_E_NOCONNECTION.into());
        }
        state.sink = None;
        Ok(())
    }

    fn RequestLock(&self, flags: u32) -> Result<HRESULT> {
        let lock = flags & TS_LF_READWRITE.0;
        if lock & TS_LF_READ.0 == 0 {
            return Err(E_INVALIDARG.into());
        }
        let sink = {
            let mut state = self.state.borrow_mut();
            let sink = state.sink.as_ref().ok_or(E_UNEXPECTED)?.sink.clone();
            if state.lock != 0 {
                if flags & TS_LF_SYNC != 0 {
                    return Ok(TS_E_SYNCHRONOUS);
                }
                state.queued |= lock;
                return Ok(TS_S_ASYNC);
            }
            state.lock = lock;
            sink
        };
        // SAFETY: the sink is live; no state borrow is held across callbacks.
        let granted = unsafe { sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(lock)) };
        loop {
            let queued = mem::take(&mut self.state.borrow_mut().queued);
            if queued == 0 {
                break;
            }
            self.state.borrow_mut().lock = queued;
            // SAFETY: as above, for the request queued during the last lock.
            let _ = unsafe { sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(queued)) };
        }
        let composition = self.composition();
        self.state.borrow_mut().lock = 0;
        self.report(composition);
        Ok(granted.map_or_else(|e| e.code(), |()| S_OK))
    }

    fn GetStatus(&self) -> Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: TS_SS_NOHIDDENTEXT,
        })
    }

    fn QueryInsert(
        &self,
        start: i32,
        end: i32,
        _: u32,
        out_start: *mut i32,
        out_end: *mut i32,
    ) -> Result<()> {
        range(&self.state.borrow().text, start, end)?;
        put(out_start, start);
        put(out_end, end);
        Ok(())
    }

    fn GetSelection(
        &self,
        index: u32,
        count: u32,
        selection: *mut TS_SELECTION_ACP,
        fetched: *mut u32,
    ) -> Result<()> {
        let state = self.read()?;
        if index != 0 && index != TS_DEFAULT_SELECTION {
            return Err(E_INVALIDARG.into());
        }
        if count == 0 || selection.is_null() {
            put(fetched, 0);
            return Ok(());
        }
        let (anchor, focus) = state.selection;
        put(
            selection,
            TS_SELECTION_ACP {
                acpStart: anchor.min(focus) as i32,
                acpEnd: anchor.max(focus) as i32,
                style: TS_SELECTIONSTYLE {
                    ase: if focus < anchor {
                        TS_AE_START
                    } else {
                        TS_AE_END
                    },
                    fInterimChar: false.into(),
                },
            },
        );
        put(fetched, 1);
        Ok(())
    }

    fn SetSelection(&self, count: u32, selection: *const TS_SELECTION_ACP) -> Result<()> {
        let mut state = self.write()?;
        if count != 1 || selection.is_null() {
            return Err(E_INVALIDARG.into());
        }
        // SAFETY: TSF passes `count` (one) readable selection.
        let selection = unsafe { &*selection };
        let range = range(&state.text, selection.acpStart, selection.acpEnd)?;
        state.selection = if selection.style.ase == TS_AE_START {
            (range.end, range.start)
        } else {
            (range.start, range.end)
        };
        Ok(())
    }

    fn GetText(
        &self,
        start: i32,
        end: i32,
        plain: PWSTR,
        plain_size: u32,
        plain_copied: *mut u32,
        runs: *mut TS_RUNINFO,
        runs_size: u32,
        runs_copied: *mut u32,
        next: *mut i32,
    ) -> Result<()> {
        let state = self.read()?;
        let range = range(&state.text, start, end)?;
        let copied = if plain.is_null() {
            0
        } else {
            range.len().min(plain_size as usize)
        };
        if copied > 0 {
            // SAFETY: TSF provides `plain_size` writable units at a non-null pointer.
            unsafe {
                std::ptr::copy_nonoverlapping(state.text[range.start..].as_ptr(), plain.0, copied);
            }
        }
        // A run-only query describes the whole range.
        let covered = if copied == 0 && runs_size > 0 {
            range.len()
        } else {
            copied
        };
        put(plain_copied, copied as u32);
        if runs_size > 0 && !runs.is_null() {
            put(
                runs,
                TS_RUNINFO {
                    uCount: covered as u32,
                    r#type: TS_RT_PLAIN,
                },
            );
            put(runs_copied, 1);
        } else {
            put(runs_copied, 0);
        }
        put(next, (range.start + covered) as i32);
        Ok(())
    }

    fn SetText(
        &self,
        _: u32,
        start: i32,
        end: i32,
        text: &PCWSTR,
        cch: u32,
    ) -> Result<TS_TEXTCHANGE> {
        let mut state = self.write()?;
        let range = range(&state.text, start, end)?;
        TextStore::replace(&mut state, range, units(text, cch)?)
    }

    fn GetFormattedText(&self, _: i32, _: i32) -> Result<IDataObject> {
        Err(E_NOTIMPL.into())
    }

    fn GetEmbedded(&self, _: i32, _: *const GUID, _: *const GUID) -> Result<IUnknown> {
        Err(TS_E_NOOBJECT.into())
    }

    fn QueryInsertEmbedded(&self, _: *const GUID, _: *const FORMATETC) -> Result<BOOL> {
        Ok(false.into())
    }

    fn InsertEmbedded(&self, _: u32, _: i32, _: i32, _: Ref<IDataObject>) -> Result<TS_TEXTCHANGE> {
        Err(E_NOTIMPL.into())
    }

    fn InsertTextAtSelection(
        &self,
        flags: u32,
        text: &PCWSTR,
        cch: u32,
        start: *mut i32,
        end: *mut i32,
        change: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        let mut state = if flags & TS_IAS_QUERYONLY != 0 {
            drop(self.read()?);
            self.state.borrow_mut()
        } else {
            self.write()?
        };
        let (anchor, focus) = state.selection;
        let selected = anchor.min(focus)..anchor.max(focus);
        if flags & TS_IAS_QUERYONLY != 0 {
            put(start, selected.start as i32);
            put(end, selected.end as i32);
            return Ok(());
        }
        let units = units(text, cch)?;
        let inserted = TextStore::replace(&mut state, selected, units)?;
        let caret = inserted.acpNewEnd as usize;
        state.selection = (caret, caret);
        if flags & TS_IAS_NOQUERY == 0 {
            put(start, inserted.acpStart);
            put(end, inserted.acpNewEnd);
        }
        put(change, inserted);
        Ok(())
    }

    fn InsertEmbeddedAtSelection(
        &self,
        _: u32,
        _: Ref<IDataObject>,
        _: *mut i32,
        _: *mut i32,
        _: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn RequestSupportedAttrs(&self, _: u32, _: u32, _: *const GUID) -> Result<()> {
        Ok(())
    }

    fn RequestAttrsAtPosition(&self, _: i32, _: u32, _: *const GUID, _: u32) -> Result<()> {
        Ok(())
    }

    fn RequestAttrsTransitioningAtPosition(
        &self,
        _: i32,
        _: u32,
        _: *const GUID,
        _: u32,
    ) -> Result<()> {
        Ok(())
    }

    fn FindNextAttrTransition(
        &self,
        _: i32,
        halt: i32,
        _: u32,
        _: *const GUID,
        _: u32,
        next: *mut i32,
        found: *mut BOOL,
        offset: *mut i32,
    ) -> Result<()> {
        put(next, halt);
        put(found, false.into());
        put(offset, 0);
        Ok(())
    }

    fn RetrieveRequestedAttrs(&self, _: u32, _: *mut TS_ATTRVAL, fetched: *mut u32) -> Result<()> {
        put(fetched, 0);
        Ok(())
    }

    fn GetEndACP(&self) -> Result<i32> {
        Ok(self.read()?.text.len() as i32)
    }

    fn GetActiveView(&self) -> Result<u32> {
        Ok(VIEW)
    }

    fn GetACPFromPoint(&self, _: u32, _: *const POINT, _: u32) -> Result<i32> {
        Err(E_NOTIMPL.into())
    }

    fn GetTextExt(
        &self,
        _: u32,
        start: i32,
        end: i32,
        rect: *mut RECT,
        clipped: *mut BOOL,
    ) -> Result<()> {
        let state = self.read()?;
        range(&state.text, start, end)?;
        // Every range reports the host's caret, which follows the composition.
        let caret = state.caret.ok_or(TS_E_NOLAYOUT)?;
        let mut origin = POINT { x: 0, y: 0 };
        // SAFETY: a live window owned by this thread; writable point.
        let _ = unsafe { ClientToScreen(self.hwnd(), &mut origin) };
        put(
            rect,
            RECT {
                left: caret.left + origin.x,
                top: caret.top + origin.y,
                right: caret.right + origin.x,
                bottom: caret.bottom + origin.y,
            },
        );
        put(clipped, false.into());
        Ok(())
    }

    fn GetScreenExt(&self, _: u32) -> Result<RECT> {
        let hwnd = self.hwnd();
        let (mut rect, mut origin) = (RECT::default(), POINT::default());
        // SAFETY: a live window owned by this thread; writable outputs.
        unsafe {
            GetClientRect(hwnd, &mut rect)?;
            let _ = ClientToScreen(hwnd, &mut origin);
        }
        Ok(RECT {
            left: origin.x,
            top: origin.y,
            right: origin.x + rect.right,
            bottom: origin.y + rect.bottom,
        })
    }

    fn GetWnd(&self, _: u32) -> Result<HWND> {
        Ok(self.hwnd())
    }
}

impl ITfContextOwnerCompositionSink_Impl for TextStore_Impl {
    fn OnStartComposition(&self, view: Ref<ITfCompositionView>) -> Result<BOOL> {
        let mut state = self.state.borrow_mut();
        if !state.reporting {
            return Ok(false.into());
        }
        state.composition = Some(view.ok()?.clone());
        Ok(true.into())
    }

    fn OnUpdateComposition(&self, _: Ref<ITfCompositionView>, _: Ref<ITfRange>) -> Result<()> {
        Ok(())
    }

    fn OnEndComposition(&self, _: Ref<ITfCompositionView>) -> Result<()> {
        self.state.borrow_mut().composition = None;
        Ok(())
    }
}
