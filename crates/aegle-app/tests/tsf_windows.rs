//! A text service reads and reconverts a native App's TextField through TSF.
//! The test is the text service: it edits the field's TSF context in edit
//! sessions, without keystrokes or an installed input method, but it creates
//! a native window: `AEGLE_TEST_COMPOSITOR=private cargo test -p aegle-app
//! --features windows,software --test tsf_windows -- --ignored`.
#![cfg(all(feature = "windows", feature = "software", target_os = "windows"))]
#![allow(unsafe_code)]

use aegle_app::{App, AppOptions, WindowOptions};
use aegle_text::{Blob, GenericFamily};
use aegle_ui::{Result, TextSystem};
use aegle_widgets::Widgets;
use std::{cell::RefCell, mem::ManuallyDrop, rc::Rc, sync::Arc, time::Duration};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::{TextServices::*, WindowsAndMessaging::*},
    },
    core::{Interface, implement, w},
};

type Session = Box<dyn Fn(u32, &ITfContext) -> windows::core::Result<()>>;

#[implement(ITfEditSession)]
struct Edit(Session, ITfContext);

impl ITfEditSession_Impl for Edit_Impl {
    fn DoEditSession(&self, cookie: u32) -> windows::core::Result<()> {
        (self.0)(cookie, &self.1)
    }
}

/// TSF rejects compositions whose owner has no termination sink.
#[implement(ITfCompositionSink)]
struct Owner;

impl ITfCompositionSink_Impl for Owner_Impl {
    fn OnCompositionTerminated(
        &self,
        _: u32,
        _: windows::core::Ref<ITfComposition>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Runs one synchronous read-write edit session on `hwnd`'s TSF context.
fn edit(hwnd: HWND, session: impl Fn(u32, &ITfContext) -> windows::core::Result<()> + 'static) {
    // SAFETY: this thread's TSF objects; the activation is balanced.
    unsafe {
        let manager: ITfThreadMgr =
            CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER).unwrap();
        let client = manager.Activate().unwrap();
        let documents = manager.EnumDocumentMgrs().unwrap();
        let context = std::iter::from_fn(|| {
            let (mut document, mut fetched) = ([None], 0);
            documents.Next(&mut document, &mut fetched).ok()?;
            document[0].take()
        })
        .filter_map(|document| document.GetTop().ok())
        .find(|context| context.GetActiveView().and_then(|v| v.GetWnd()) == Ok(hwnd))
        .expect("the window has no TSF context");
        let session: ITfEditSession = Edit(Box::new(session), context.clone()).into();
        context
            .RequestEditSession(client, &session, TF_ES_SYNC | TF_ES_READWRITE)
            .unwrap()
            .ok()
            .unwrap();
        manager.Deactivate().unwrap();
    }
}

/// The whole document and the caret, in UTF-16 units.
fn read(hwnd: HWND) -> (String, i32) {
    let out = Rc::new(RefCell::new((String::new(), 0)));
    let sink = out.clone();
    edit(hwnd, move |cookie, context| {
        // SAFETY: inside an edit session of `context`; the selection's range
        // is taken exactly once.
        unsafe {
            let all = context.GetStart(cookie)?;
            all.ShiftEnd(cookie, i32::MAX, &mut 0, std::ptr::null())?;
            let (mut units, mut length) = ([0u16; 64], 0);
            all.GetText(cookie, 0, &mut units, &mut length)?;
            let (mut selection, mut fetched) = ([TF_SELECTION::default()], 0);
            context.GetSelection(cookie, TF_DEFAULT_SELECTION, &mut selection, &mut fetched)?;
            let caret = ManuallyDrop::take(&mut selection[0].range).unwrap();
            let (mut start, mut count) = (0, 0);
            caret
                .cast::<ITfRangeACP>()?
                .GetExtent(&mut start, &mut count)?;
            *sink.borrow_mut() = (String::from_utf16_lossy(&units[..length as usize]), start);
        }
        Ok(())
    });
    out.take()
}

fn run(app: &App) -> Result {
    for _ in 0..10 {
        app.dispatch(Some(Duration::from_millis(20)))?;
    }
    Ok(())
}

#[test]
#[ignore = "creates a native window; run on a dedicated Windows test desktop"]
fn text_services_read_and_reconvert_a_text_field() -> Result {
    assert_eq!(
        std::env::var("AEGLE_TEST_COMPOSITOR").as_deref(),
        Ok("private")
    );
    let mut fonts = TextSystem::new();
    let family = fonts.register_fonts(Blob::new(Arc::new(
        include_bytes!("../../../tests/assets/aegle-test-cjk.otf").as_slice(),
    )))?[0]
        .0;
    fonts
        .collection_mut()
        .set_generic_families(GenericFamily::SansSerif, [family].into_iter());
    let app = App::with_fonts(fonts, AppOptions::default())?;
    let window = app.window_with_options(
        "Aegle TSF field",
        WindowOptions {
            width: 320,
            height: 120,
            ..Default::default()
        },
    )?;
    let field = window.text_field("你好世界");
    field.focus();
    run(&app)?;
    // SAFETY: looks up this process's uniquely titled window.
    let hwnd = unsafe { FindWindowW(None, w!("Aegle TSF field")) }.unwrap();

    // The text service reads the field's text and caret as its document.
    assert_eq!(read(hwnd), ("你好世界".into(), 4));

    // Reconversion: a composition over "世界" before the caret removes it
    // from the committed value and shows it as preedit.
    let composition = Rc::new(RefCell::new(None::<ITfComposition>));
    let started = composition.clone();
    edit(hwnd, move |cookie, context| {
        // SAFETY: inside an edit session of `context`.
        unsafe {
            let range = context.GetStart(cookie)?;
            range.cast::<ITfRangeACP>()?.SetExtent(2, 2)?;
            let owner: ITfCompositionSink = Owner.into();
            let composing = context.cast::<ITfContextComposition>()?;
            *started.borrow_mut() = Some(composing.StartComposition(cookie, &range, &owner)?);
        }
        Ok(())
    });
    run(&app)?;
    assert_eq!(field.text(), "你好");

    // New text replaces the preedit and commits when the composition ends.
    let ending = composition.clone();
    edit(hwnd, move |cookie, _| {
        let units: Vec<u16> = "时节".encode_utf16().collect();
        // SAFETY: inside an edit session; the composition is ended once.
        unsafe {
            let composition = ending.take().unwrap();
            composition.GetRange()?.SetText(cookie, 0, &units)?;
            composition.EndComposition(cookie)
        }
    });
    run(&app)?;
    assert_eq!(field.text(), "你好时节");

    // The field's republished state is what the text service reads next.
    assert_eq!(read(hwnd), ("你好时节".into(), 4));
    window.close()?;
    Ok(())
}
