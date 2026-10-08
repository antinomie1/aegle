//! Windows: one thread owns a hidden window that receives tray and hotkey
//! messages, and shows file dialogs. Commands arrive over a channel and run
//! between messages, outside the window procedure, so a dialog's modal loop
//! can still dispatch tray messages.
#![allow(unsafe_code)]

use std::{
    cell::RefCell,
    io,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle,
};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Input::KeyboardAndMouse::{
                HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
                RegisterHotKey, UnregisterHotKey,
            },
            Shell::{
                NIM_ADD, NIN_BALLOONHIDE, NIN_BALLOONTIMEOUT, NIN_BALLOONUSERCLICK, NIN_SELECT,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
                HICON, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
                WINDOW_EX_STYLE, WM_APP, WM_CONTEXTMENU, WM_HOTKEY, WM_LBUTTONUP, WNDCLASSW,
                WS_OVERLAPPED,
            },
        },
    },
    core::w,
};

mod files;
mod tray;

use tray::{Entry, entries, popup};

use crate::{Event, FileDialog, Notification, Shortcut, Tray};

pub(crate) type Sink = Box<dyn Fn(Event) + Send + Sync>;

/// Wakes the thread to run queued commands.
const COMMAND: u32 = WM_APP;
/// Tray icon callbacks.
const TRAY: u32 = WM_APP + 1;
/// Ends the thread.
const QUIT: u32 = WM_APP + 2;
/// A tray icon chosen with the keyboard: `NIN_SELECT | NINF_KEY`.
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;

/// Windows errors as I/O errors, by their message.
fn io(error: windows::core::Error) -> io::Error {
    io::Error::other(error.message())
}

/// A [`Tray`] with BGRA pixels, owned by the thread.
struct OwnedTray {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
    tooltip: String,
    menu: Vec<Entry>,
}

enum Command {
    Files {
        request: u32,
        save: bool,
        title: Vec<u16>,
        filters: Vec<(Vec<u16>, Vec<u16>)>,
        multiple: bool,
        directory: bool,
        name: Vec<u16>,
        folder: Option<Vec<u16>>,
    },
    Notify {
        id: u32,
        summary: String,
        body: String,
    },
    Tray(Option<OwnedTray>),
    Shortcuts(Vec<(String, HOT_KEY_MODIFIERS, u32)>),
}

/// A NUL-terminated UTF-16 copy.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub(crate) struct Desktop {
    commands: Sender<Command>,
    /// The hidden window, as an address so the handle stays `Send`.
    hwnd: isize,
    next: AtomicU32,
    thread: Option<JoinHandle<()>>,
}

impl Desktop {
    pub fn new(_app_id: &str, sink: Sink) -> io::Result<Self> {
        let (commands, receiver) = channel();
        let (ready, created) = channel();
        let sink = Arc::new(sink);
        let thread = std::thread::Builder::new()
            .name("aegle-desktop".into())
            .spawn(move || run(sink, receiver, ready))?;
        let hwnd = created
            .recv()
            .map_err(|_| io::Error::other("desktop thread ended"))?
            .map_err(io)?;
        Ok(Self {
            commands,
            hwnd,
            next: AtomicU32::new(0),
            thread: Some(thread),
        })
    }

    fn send(&self, command: Command) -> io::Result<()> {
        self.commands
            .send(command)
            .map_err(|_| io::Error::other("desktop thread ended"))?;
        // SAFETY: posting to a window of another thread is allowed; the
        // window lives until this Desktop is dropped.
        unsafe { PostMessageW(Some(HWND(self.hwnd as _)), COMMAND, WPARAM(0), LPARAM(0)) }
            .map_err(io)
    }

    fn id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn file_dialog(&self, dialog: &FileDialog<'_>, save: bool) -> io::Result<u32> {
        let request = self.id();
        let filters = dialog
            .filters
            .iter()
            .map(|(name, patterns)| (wide(name), wide(&patterns.join(";"))))
            .collect();
        self.send(Command::Files {
            request,
            save,
            title: wide(dialog.title),
            filters,
            multiple: dialog.multiple,
            directory: dialog.directory,
            name: wide(dialog.name),
            folder: dialog.folder.map(|f| wide(&f.to_string_lossy())),
        })?;
        Ok(request)
    }

    pub fn notify(&self, notification: &Notification<'_>) -> io::Result<u32> {
        let id = self.id();
        self.send(Command::Notify {
            id,
            summary: notification.summary.into(),
            body: notification.body.into(),
        })?;
        Ok(id)
    }

    pub fn set_tray(&self, tray: Option<&Tray<'_>>) -> io::Result<()> {
        self.send(Command::Tray(tray.map(|tray| {
            let icon = tray.icon;
            let bgra = icon
                .rgba
                .chunks(4)
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect();
            OwnedTray {
                width: icon.width,
                height: icon.height,
                bgra,
                tooltip: tray.tooltip.into(),
                menu: entries(tray.menu),
            }
        })))
    }

    pub fn bind_shortcuts(&self, shortcuts: &[Shortcut<'_>]) -> io::Result<()> {
        let keys = shortcuts.iter().map(|shortcut| {
            let trigger = shortcut.trigger;
            let mut modifiers = MOD_NOREPEAT;
            for (on, flag) in [
                (trigger.ctrl, MOD_CONTROL),
                (trigger.alt, MOD_ALT),
                (trigger.shift, MOD_SHIFT),
                (trigger.logo, MOD_WIN),
            ] {
                if on {
                    modifiers |= flag;
                }
            }
            (shortcut.id.into(), modifiers, trigger.virtual_key().into())
        });
        self.send(Command::Shortcuts(keys.collect()))
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        // SAFETY: as in `send`.
        let _ = unsafe { PostMessageW(Some(HWND(self.hwnd as _)), QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The thread's state, reached from the window procedure.
struct Worker {
    hwnd: HWND,
    sink: Arc<Sink>,
    /// The icon handle, and whether the user set it (else a balloon added it).
    icon: Option<(HICON, bool)>,
    tooltip: String,
    menu: Vec<Entry>,
    /// The notification its balloon shows.
    balloon: Option<u32>,
    shortcuts: Vec<String>,
    /// Explorer broadcasts it after restarting, when icons must be re-added.
    taskbar_created: u32,
}

thread_local! {
    // The initializer is const already; the lint misreads the macro.
    #[allow(clippy::missing_const_for_thread_local)]
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

fn run(
    sink: Arc<Sink>,
    commands: Receiver<Command>,
    ready: Sender<Result<isize, windows::core::Error>>,
) {
    let created = (|| {
        // SAFETY: plain calls on this new thread; the class lives for the
        // process, and `procedure` handles every message of its windows.
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let instance = GetModuleHandleW(None)?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: w!("Aegle.Desktop.v1"),
                ..Default::default()
            };
            RegisterClassW(&class);
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("Aegle.Desktop.v1"),
                w!(""),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance.into()),
                None,
            )
        }
    })();
    let hwnd = match created {
        Ok(hwnd) => hwnd,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    // SAFETY: a constant string.
    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    WORKER.set(Some(Worker {
        hwnd,
        sink: sink.clone(),
        icon: None,
        tooltip: String::new(),
        menu: Vec::new(),
        balloon: None,
        shortcuts: Vec::new(),
        taskbar_created,
    }));
    let _ = ready.send(Ok(hwnd.0 as isize));
    let mut message = MSG::default();
    // SAFETY: the standard loop of this thread's windows.
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        // SAFETY: as above.
        unsafe { DispatchMessageW(&message) };
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Files {
                    request,
                    save,
                    title,
                    filters,
                    multiple,
                    directory,
                    name,
                    folder,
                } => {
                    let shown = files::show(
                        save,
                        &title,
                        &filters,
                        multiple,
                        directory,
                        &name,
                        folder.as_deref(),
                    );
                    let paths = match shown {
                        Ok(paths) => paths,
                        Err(error) => {
                            sink(Event::Unavailable(format!(
                                "file dialog: {}",
                                error.message()
                            )));
                            None
                        }
                    };
                    sink(Event::Files { request, paths });
                }
                command => {
                    with(|worker| worker.command(command));
                }
            }
        }
    }
    with(Worker::close);
}

/// Runs `f` on the thread's worker. Nothing that dispatches messages may run
/// inside `f`, since the window procedure borrows the worker too.
fn with<T>(f: impl FnOnce(&mut Worker) -> T) -> Option<T> {
    WORKER.with_borrow_mut(|worker| worker.as_mut().map(f))
}

impl Worker {
    fn emit(&self, event: Event) {
        (self.sink)(event);
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Files { .. } => unreachable!("run handles file dialogs"),
            Command::Tray(None) => {
                if self.icon.is_some_and(|(_, user)| user) {
                    self.remove();
                }
                self.menu.clear();
            }
            Command::Tray(Some(tray)) => self.set(tray),
            Command::Notify { id, summary, body } => self.balloon(id, &summary, &body),
            Command::Shortcuts(keys) => {
                self.unregister();
                for (index, (id, modifiers, key)) in keys.into_iter().enumerate() {
                    // SAFETY: plain registration for this window.
                    let result = unsafe {
                        RegisterHotKey(Some(self.hwnd), index as i32 + 1, modifiers, key)
                    };
                    if let Err(error) = result {
                        self.emit(Event::Unavailable(format!(
                            "shortcut {id}: {}",
                            error.message()
                        )));
                    }
                    self.shortcuts.push(id);
                }
            }
        }
    }

    fn unregister(&mut self) {
        for index in 0..self.shortcuts.len() {
            // SAFETY: ids this window registered.
            let _ = unsafe { UnregisterHotKey(Some(self.hwnd), index as i32 + 1) };
        }
        self.shortcuts.clear();
    }

    fn close(&mut self) {
        self.remove();
        self.unregister();
    }
}

/// The hidden window's procedure.
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let low = |value: usize| (value & 0xFFFF) as u16 as i16 as i32;
    match message {
        QUIT => {
            with(Worker::close);
            // SAFETY: this thread's own window and loop.
            unsafe {
                let _ = DestroyWindow(hwnd);
                PostQuitMessage(0);
            }
        }
        TRAY => {
            // NOTIFYICON_VERSION_4: the event in the low word of `l`, the
            // anchor point in `w`.
            let event = (l.0 as u32) & 0xFFFF;
            let (x, y) = (low(w.0), low(w.0 >> 16));
            if event == WM_CONTEXTMENU {
                popup(x, y);
                return LRESULT(0);
            }
            with(|worker| match event {
                WM_LBUTTONUP | NIN_SELECT | NIN_KEYSELECT => worker.emit(Event::TrayActivated),
                NIN_BALLOONUSERCLICK | NIN_BALLOONTIMEOUT | NIN_BALLOONHIDE => {
                    let Some(notification) = worker.balloon.take() else {
                        return;
                    };
                    if event == NIN_BALLOONUSERCLICK {
                        let action = "default".into();
                        worker.emit(Event::NotificationAction {
                            notification,
                            action,
                        });
                    }
                    worker.emit(Event::NotificationClosed { notification });
                    if worker.icon.is_some_and(|(_, user)| !user) {
                        worker.remove();
                    }
                }
                _ => {}
            });
        }
        WM_HOTKEY => {
            with(|worker| {
                if let Some(id) = worker.shortcuts.get(w.0.wrapping_sub(1)) {
                    worker.emit(Event::Shortcut { id: id.clone() });
                }
            });
        }
        COMMAND => {}
        message
            if WORKER.with_borrow(|w| w.as_ref().is_some_and(|w| w.taskbar_created == message)) =>
        {
            with(|worker| {
                if worker.icon.is_some() {
                    worker.show(NIM_ADD);
                }
            });
        }
        // SAFETY: unhandled messages forwarded unchanged.
        _ => return unsafe { DefWindowProcW(hwnd, message, w, l) },
    }
    LRESULT(0)
}
