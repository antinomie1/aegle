//! Text Services Framework input: one activated thread manager per UI thread
//! and, per window, a document whose only context is the window's
//! [`TextStore`]. A window without an enabled editor focuses an empty
//! document, so no input method composes there.
#![allow(unsafe_code)]

use crate::{Error, Event, ImeEvent, ImeRequest, native::Native, text_store::TextStore};
use std::{cell::Cell, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, RECT},
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::{Input::KeyboardAndMouse::GetFocus, TextServices::*},
    },
    core::{ComObject, Interface},
};

/// The calling thread's activated TSF.
pub(crate) struct Tsf {
    manager: ITfThreadMgr,
    client: u32,
    /// Focused by windows without an enabled editor.
    empty: ITfDocumentMgr,
}

impl Tsf {
    /// Activates TSF on a thread that has initialized COM.
    pub fn activate() -> Result<Rc<Self>, Error> {
        // SAFETY: COM is initialized on this thread; the manager is
        // deactivated in Drop on the same thread.
        unsafe {
            let manager: ITfThreadMgr =
                CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
            let client = manager.Activate()?;
            match manager.CreateDocumentMgr() {
                Ok(empty) => Ok(Rc::new(Self {
                    manager,
                    client,
                    empty,
                })),
                Err(error) => {
                    let _ = manager.Deactivate();
                    Err(error.into())
                }
            }
        }
    }
}

impl Drop for Tsf {
    fn drop(&mut self) {
        // SAFETY: balances Activate on the owning thread.
        unsafe {
            let _ = self.manager.Deactivate();
        }
    }
}

/// One window's TSF document and its enabled editor session.
pub(crate) struct Session {
    tsf: Rc<Tsf>,
    hwnd: HWND,
    document: ITfDocumentMgr,
    services: ITfContextOwnerCompositionServices,
    store: ComObject<TextStore>,
    enabled: Cell<bool>,
}

impl Session {
    /// Creates a disabled session for a created window.
    pub fn new(tsf: &Rc<Tsf>, native: &Rc<Native>) -> Result<Self, Error> {
        let store = ComObject::new(TextStore::new(Rc::downgrade(native)));
        // SAFETY: TSF objects of this thread; the store outlives its context
        // through COM reference counting.
        unsafe {
            let document = tsf.manager.CreateDocumentMgr()?;
            let (mut context, mut cookie) = (None, 0);
            document.CreateContext(
                tsf.client,
                0,
                &store.to_interface::<ITextStoreACP>(),
                &mut context,
                &mut cookie,
            )?;
            let context: ITfContext =
                context.ok_or_else(|| Error::Backend("TSF created no context".into()))?;
            document.Push(&context)?;
            let session = Self {
                tsf: tsf.clone(),
                hwnd: native.hwnd.get(),
                document,
                services: context.cast()?,
                store,
                enabled: Cell::new(false),
            };
            session.associate(&session.tsf.empty)?;
            Ok(session)
        }
    }

    /// Whether an input method is composing in this window.
    pub fn composing(&self) -> bool {
        self.store.composing()
    }

    /// Whether an editor session is enabled.
    pub fn enabled(&self) -> bool {
        self.enabled.get()
    }

    /// Focuses the window's document; true when the session was disabled.
    fn enable(&self) -> Result<bool, Error> {
        if self.enabled.replace(true) {
            return Ok(false);
        }
        self.store.begin();
        self.associate(&self.document)?;
        Ok(true)
    }

    /// Ends any composition without reporting it and focuses the empty
    /// document. The host cancels its own preedit.
    pub fn disable(&self) {
        if !self.enabled.replace(false) {
            return;
        }
        self.store.stop();
        if self.store.composing() {
            // SAFETY: a live context of this thread; TSF calls back into the
            // store, which no longer reports.
            unsafe {
                let _ = self
                    .services
                    .TerminateComposition(None::<&ITfCompositionView>);
            }
        }
        self.store.forget_composition();
        let _ = self.associate(&self.tsf.empty);
    }

    fn associate(&self, document: &ITfDocumentMgr) -> windows::core::Result<()> {
        // SAFETY: the window and document belong to this thread. The previous
        // association comes back, and NULL arrives as an error carrying S_OK.
        unsafe {
            if let Err(error) = self.tsf.manager.AssociateFocus(self.hwnd, document)
                && error.code().is_err()
            {
                return Err(error);
            }
            if GetFocus() == self.hwnd {
                self.tsf.manager.SetFocus(document)?;
            }
        }
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.disable();
        // SAFETY: detaches this window and releases the context it owns.
        unsafe {
            let _ = self
                .tsf
                .manager
                .AssociateFocus(self.hwnd, None::<&ITfDocumentMgr>);
            let _ = self.document.Pop(TF_POPF_ALL);
        }
    }
}

pub(crate) fn configure(native: &Native, request: Option<ImeRequest>) -> Result<(), Error> {
    let Some(request) = request else {
        native.cancel_ime();
        return Ok(());
    };
    let [left, top, right, bottom] = request.validate(native.info.get().scale)?;
    let session = native.tsf.borrow();
    let session = session.as_ref().ok_or(Error::ImeUnavailable)?;
    let caret = RECT {
        left,
        top,
        right,
        bottom,
    };
    // The host's state replaces what an ended session left in the document
    // before reporting resumes and an input method can read it.
    session.store.publish(
        request.surrounding.as_deref(),
        (request.anchor, request.cursor),
        caret,
    );
    if session.enable()? && native.info.get().active {
        native.emit(Event::Ime {
            window: native.id,
            event: ImeEvent::Entered,
        });
    }
    Ok(())
}
