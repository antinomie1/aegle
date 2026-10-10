//! The TSF text store through the real TSF runtime. The test acts as a text
//! service does: it requests edit sessions on the window's context, reads the
//! document and selection, composes, reconverts committed text and asks for
//! candidate geometry. No input method or keystrokes are involved, but it
//! creates a native window:
//! `cargo test -p aegle-platform-win32 --test tsf -- --ignored`.
#![cfg(windows)]
#![allow(unsafe_code)]
use aegle_platform_win32::{
    Event, ImeEvent, ImeRequest, ImeUpdate, PixelSize, Preedit, Win32, WindowId, WindowOptions,
};
use aegle_types::Rect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{cell::RefCell, mem::ManuallyDrop, rc::Rc};
use windows::{
    Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::Gdi::ClientToScreen,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::TextServices::*,
    },
    core::{Interface, Result, implement},
};

#[implement(ITfEditSession)]
struct Edit(Box<dyn Fn(u32) -> Result<()>>);

impl ITfEditSession_Impl for Edit_Impl {
    fn DoEditSession(&self, cookie: u32) -> Result<()> {
        (self.0)(cookie)
    }
}

/// A composition owner's termination sink. TSF rejects a composition started
/// without one (E_INVALIDARG), although the parameter is documented optional.
#[implement(ITfCompositionSink)]
struct Terminated;

impl ITfCompositionSink_Impl for Terminated_Impl {
    fn OnCompositionTerminated(&self, _: u32, _: windows::core::Ref<ITfComposition>) -> Result<()> {
        Ok(())
    }
}

/// A text-service client of this thread's TSF, editing one window's context.
struct Service {
    manager: ITfThreadMgr,
    client: u32,
    context: ITfContext,
    composition: Rc<RefCell<Option<ITfComposition>>>,
}

impl Service {
    fn new(hwnd: HWND) -> Result<Self> {
        // SAFETY: this thread's TSF objects; activation is balanced in Drop.
        unsafe {
            let manager: ITfThreadMgr =
                CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER)?;
            let client = manager.Activate()?;
            let documents = manager.EnumDocumentMgrs()?;
            let mut context = None;
            loop {
                let (mut document, mut fetched) = ([None], 0);
                documents.Next(&mut document, &mut fetched)?;
                let Some(document) = document[0].take().filter(|_| fetched == 1) else {
                    break;
                };
                // Other documents of the thread, such as the empty one of
                // windows without an editor, have no context of this window.
                if let Ok(top) = document.GetTop()
                    && top.GetActiveView().and_then(|view| view.GetWnd()) == Ok(hwnd)
                {
                    context = Some(top);
                }
            }
            Ok(Self {
                manager,
                client,
                context: context.expect("the window has no TSF context"),
                composition: Rc::default(),
            })
        }
    }

    /// Runs a synchronous read-write edit session.
    fn edit(
        &self,
        f: impl Fn(u32, &ITfContext, &RefCell<Option<ITfComposition>>) -> Result<()> + 'static,
    ) {
        let (context, composition) = (self.context.clone(), self.composition.clone());
        let session: ITfEditSession =
            Edit(Box::new(move |cookie| f(cookie, &context, &composition))).into();
        // SAFETY: a live context; the session runs before this call returns.
        let result = unsafe {
            self.context
                .RequestEditSession(self.client, &session, TF_ES_SYNC | TF_ES_READWRITE)
        };
        result.unwrap().ok().unwrap();
    }

    /// The document and the selection as UTF-16 offsets.
    fn read(&self) -> (String, (i32, i32)) {
        let out = Rc::new(RefCell::new((String::new(), (0, 0))));
        let sink = out.clone();
        self.edit(move |cookie, context, _| {
            let whole = range(cookie, context, 0, i32::MAX)?;
            let (mut units, mut length) = ([0u16; 256], 0);
            // SAFETY: a range of this context inside its edit session.
            unsafe { whole.GetText(cookie, 0, &mut units, &mut length)? };
            let selection = selection(cookie, context)?;
            *sink.borrow_mut() = (
                String::from_utf16_lossy(&units[..length as usize]),
                extent(&selection)?,
            );
            Ok(())
        });
        out.take()
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        // SAFETY: balances Activate.
        unsafe {
            let _ = self.manager.Deactivate();
        }
    }
}

/// A range at UTF-16 offsets, clamped to the document.
fn range(cookie: u32, context: &ITfContext, start: i32, length: i32) -> Result<ITfRange> {
    // SAFETY: inside an edit session of `context`.
    unsafe {
        let range = context.GetStart(cookie)?;
        if length == i32::MAX {
            let mut moved = 0;
            range.ShiftEnd(cookie, i32::MAX, &mut moved, std::ptr::null())?;
            return Ok(range);
        }
        range.cast::<ITfRangeACP>()?.SetExtent(start, length)?;
        Ok(range)
    }
}

fn extent(range: &ITfRange) -> Result<(i32, i32)> {
    let (mut start, mut length) = (0, 0);
    // SAFETY: ranges of an ACP text store expose their offsets.
    unsafe {
        range
            .cast::<ITfRangeACP>()?
            .GetExtent(&mut start, &mut length)?
    };
    Ok((start, start + length))
}

fn selection(cookie: u32, context: &ITfContext) -> Result<ITfRange> {
    let (mut selection, mut fetched) = ([TF_SELECTION::default()], 0);
    // SAFETY: inside an edit session; the fetched range is taken exactly once.
    unsafe {
        context.GetSelection(cookie, TF_DEFAULT_SELECTION, &mut selection, &mut fetched)?;
        assert_eq!(fetched, 1);
        Ok(ManuallyDrop::take(&mut selection[0].range).unwrap())
    }
}

fn select(cookie: u32, context: &ITfContext, range: &ITfRange) -> Result<()> {
    let selection = TF_SELECTION {
        range: ManuallyDrop::new(Some(range.clone())),
        style: TF_SELECTIONSTYLE {
            ase: TF_AE_END,
            fInterimChar: false.into(),
        },
    };
    // SAFETY: inside an edit session; the cloned range is released below.
    let result = unsafe { context.SetSelection(cookie, std::slice::from_ref(&selection)) };
    drop(ManuallyDrop::into_inner(selection.range));
    result
}

/// Starts a composition over UTF-16 `start..end` and writes `text` into it.
fn compose(service: &Service, start: i32, end: i32, text: &'static str) {
    service.edit(move |cookie, context, composition| {
        let range = range(cookie, context, start, end - start)?;
        // SAFETY: inside an edit session of `context`.
        unsafe {
            let started = context.cast::<ITfContextComposition>()?.StartComposition(
                cookie,
                &range,
                &ITfCompositionSink::from(Terminated),
            )?;
            *composition.borrow_mut() = Some(started);
        }
        if !text.is_empty() {
            write(cookie, context, composition, text)?;
        }
        Ok(())
    });
}

/// Replaces the composition's text and puts the caret after it.
fn write(
    cookie: u32,
    context: &ITfContext,
    composition: &RefCell<Option<ITfComposition>>,
    text: &str,
) -> Result<()> {
    let units: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: inside an edit session of `context`.
    unsafe {
        let range = composition.borrow().as_ref().unwrap().GetRange()?;
        range.SetText(cookie, 0, &units)?;
        let caret = range.Clone()?;
        caret.Collapse(cookie, TF_ANCHOR_END)?;
        select(cookie, context, &caret)
    }
}

fn update(service: &Service, text: &'static str) {
    service.edit(move |cookie, context, composition| write(cookie, context, composition, text));
}

fn end(service: &Service) {
    service.edit(|cookie, _, composition| {
        // SAFETY: inside an edit session; ending keeps the composed text.
        unsafe { composition.take().unwrap().EndComposition(cookie) }
    });
}

fn updates(backend: &mut Win32, window: WindowId) -> Vec<ImeUpdate> {
    let mut updates = Vec::new();
    while let Some(event) = backend.next_event() {
        match event {
            Event::Ime {
                window: w,
                event: ImeEvent::Update(update),
            } if w == window => updates.push(update),
            Event::Error(error) => panic!("{error}"),
            _ => {}
        }
    }
    updates
}

fn preedit(text: &str, cursor: Option<(usize, usize)>, delete_after: usize) -> ImeUpdate {
    ImeUpdate {
        delete_after,
        preedit: Preedit {
            text: text.into(),
            cursor,
        },
        ..Default::default()
    }
}

fn commit(text: &str) -> ImeUpdate {
    ImeUpdate {
        commit: Some(text.into()),
        ..Default::default()
    }
}

fn request(text: &str, caret: usize) -> ImeRequest {
    ImeRequest {
        surrounding: Some(text.into()),
        cursor: caret,
        anchor: caret,
        cursor_rect: Rect::new(10.0, 20.0, 1.5, 16.0),
    }
}

#[test]
#[ignore = "creates a native window; run on a dedicated Windows test desktop"]
fn text_services_compose_reconvert_and_resync()
-> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut backend = Win32::connect()?;
    assert!(backend.ime_available());
    let window = backend.create_window(WindowOptions {
        title: "Aegle TSF",
        size: PixelSize {
            width: 160,
            height: 80,
        },
        ..Default::default()
    })?;
    let lease = backend.window_surface(window)?;
    let RawWindowHandle::Win32(handle) = lease.window_handle().unwrap().as_raw() else {
        unreachable!()
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    backend.configure_ime(window, Some(request("你好世界", 6)))?;
    let service = Service::new(hwnd).map_err(aegle_platform_win32::Error::from)?;

    // Text services read the surrounding excerpt and the caret in UTF-16.
    assert_eq!(service.read(), ("你好世界".into(), (2, 2)));
    // Candidate windows are placed at the caret, in screen pixels.
    let geometry = Rc::new(RefCell::new(RECT::default()));
    let sink = geometry.clone();
    service.edit(move |cookie, context, _| {
        let caret = range(cookie, context, 2, 0)?;
        let mut clipped = false.into();
        // SAFETY: inside an edit session of `context`.
        unsafe {
            context.GetActiveView()?.GetTextExt(
                cookie,
                &caret,
                &mut *sink.borrow_mut(),
                &mut clipped,
            )
        }
    });
    let scale = backend.window_info(window)?.scale;
    let [left, top, right, bottom] = request("", 0).validate(scale)?;
    let mut origin = POINT::default();
    // SAFETY: a live window of this thread.
    let _ = unsafe { ClientToScreen(hwnd, &mut origin) };
    let expected = RECT {
        left: left + origin.x,
        top: top + origin.y,
        right: right + origin.x,
        bottom: bottom + origin.y,
    };
    assert_eq!(*geometry.borrow(), expected);
    assert!(updates(&mut backend, window).is_empty());

    // A composition at the caret is preedit until it ends, then a commit.
    compose(&service, 2, 2, "zhong");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("zhong", Some((5, 5)), 0)]
    );
    update(&service, "中");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("中", Some((3, 3)), 0)]
    );
    end(&service);
    assert_eq!(updates(&mut backend, window), [commit("中")]);
    assert_eq!(service.read(), ("你好中世界".into(), (3, 3)));
    backend.configure_ime(window, Some(request("你好中世界", 9)))?;

    // Reconversion: composing over committed text after the caret deletes it
    // around the caret and shows it as preedit until the new text commits.
    compose(&service, 3, 5, "");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("世界", Some((0, 0)), 6)]
    );
    update(&service, "时节");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("时节", Some((6, 6)), 0)]
    );
    end(&service);
    assert_eq!(updates(&mut backend, window), [commit("时节")]);
    backend.configure_ime(window, Some(request("你好中时节", 15)))?;

    // Surrogate pairs that differ only in their second unit stay whole.
    compose(&service, 5, 5, "🙂");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("🙂", Some((4, 4)), 0)]
    );
    update(&service, "😀");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("😀", Some((4, 4)), 0)]
    );
    end(&service);
    assert_eq!(updates(&mut backend, window), [commit("😀")]);
    backend.configure_ime(window, Some(request("你好中时节😀", 19)))?;
    // Committed text replaced in place, before the caret, without composing.
    service.edit(|cookie, context, _| {
        let units: Vec<u16> = "🙂".encode_utf16().collect();
        // SAFETY: inside an edit session of `context`.
        unsafe { range(cookie, context, 5, 2)?.SetText(cookie, 0, &units) }
    });
    let replaced = ImeUpdate {
        delete_before: 4,
        ..commit("🙂")
    };
    assert_eq!(updates(&mut backend, window), [replaced]);

    // Emptying a composition deletes its preedit; ending it adds nothing.
    compose(&service, 7, 7, "x");
    assert_eq!(
        updates(&mut backend, window),
        [preedit("x", Some((1, 1)), 0)]
    );
    update(&service, "");
    assert_eq!(updates(&mut backend, window), [commit("")]);
    end(&service);
    assert!(updates(&mut backend, window).is_empty());

    // A composition the host cannot place after its caret is an error, not
    // a silently misplaced edit; the host's next publish restores the store.
    compose(&service, 0, 0, "x");
    let mut errors = 0;
    while let Some(event) = backend.next_event() {
        match event {
            Event::Error(_) => errors += 1,
            Event::Ime {
                event: ImeEvent::Update(update),
                ..
            } => panic!("{update:?}"),
            _ => {}
        }
    }
    assert_eq!(errors, 1);

    // Disabling ends the composition without reporting it; re-enabling
    // replaces what the text service left with the host's text.
    backend.configure_ime(window, None)?;
    assert!(updates(&mut backend, window).is_empty());
    backend.configure_ime(window, Some(request("新的", 3)))?;
    assert_eq!(service.read(), ("新的".into(), (1, 1)));
    assert!(updates(&mut backend, window).is_empty());

    drop(service);
    backend.remove_window(window)?;
    drop(lease);
    Ok(())
}
