//! Desktop services outside windows: file dialogs, notifications, a tray
//! icon with a menu, and global shortcuts.
//!
//! A [`Desktop`] runs a background thread that talks to the desktop and
//! passes each [`Event`] to the sink given to [`Desktop::new`], on that
//! thread; a UI usually forwards them to its own thread, as
//! `aegle_app::App::desktop` does. Requests return at once; their results
//! arrive as events.
//!
//! | Service | Linux | Windows |
//! |---|---|---|
//! | File dialogs | `org.freedesktop.portal.FileChooser` | `IFileOpenDialog`, `IFileSaveDialog` |
//! | Notifications | `org.freedesktop.Notifications` | a tray balloon (a toast on Windows 10 and later) |
//! | Tray | `StatusNotifierItem` with `com.canonical.dbusmenu` | `Shell_NotifyIcon` and a popup menu |
//! | Global shortcuts | `org.freedesktop.portal.GlobalShortcuts` | `RegisterHotKey` |
//!
//! A service the desktop does not offer reports [`Event::Unavailable`]
//! instead of failing silently.

use std::path::PathBuf;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows as imp;

mod shortcut;
pub use shortcut::Trigger;

/// What the desktop reports. Ids are those the requests returned.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A file dialog closed: the chosen paths, or `None` when cancelled.
    Files {
        /// The dialog's request.
        request: u32,
        /// Chosen files or folders.
        paths: Option<Vec<PathBuf>>,
    },
    /// The user chose an action of a notification; clicking its body is
    /// the action `"default"`.
    NotificationAction {
        /// The notification.
        notification: u32,
        /// The action's key.
        action: String,
    },
    /// A notification went away.
    NotificationClosed {
        /// The notification.
        notification: u32,
    },
    /// The tray icon was activated, usually by a primary click.
    TrayActivated,
    /// A tray menu item was chosen.
    TrayMenu {
        /// The item's id.
        item: u32,
    },
    /// A global shortcut was pressed.
    Shortcut {
        /// The shortcut's id.
        id: String,
    },
    /// A service is missing or refused a request, such as a desktop without
    /// a global shortcuts portal; the text says which and why.
    Unavailable(String),
}

/// A file dialog.
#[derive(Clone, Debug, Default)]
pub struct FileDialog<'a> {
    /// The window title.
    pub title: &'a str,
    /// Named filters with their glob patterns, such as
    /// `("Images", &["*.png", "*.jpg"])`; the first one is selected.
    pub filters: &'a [(&'a str, &'a [&'a str])],
    /// Opening: whether several may be chosen.
    pub multiple: bool,
    /// Opening: whether folders are chosen instead of files.
    pub directory: bool,
    /// Saving: the suggested file name.
    pub name: &'a str,
    /// The folder it starts in.
    pub folder: Option<&'a std::path::Path>,
}

/// A notification.
#[derive(Clone, Debug, Default)]
pub struct Notification<'a> {
    /// The one-line summary.
    pub summary: &'a str,
    /// The body text.
    pub body: &'a str,
    /// Buttons as `(key, label)`; Windows shows none and reports only the
    /// `"default"` action.
    pub actions: &'a [(&'a str, &'a str)],
}

/// Straight (not premultiplied) RGBA pixels, row by row.
#[derive(Clone, Copy, Debug)]
pub struct Icon<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: &'a [u8],
}

/// A tray menu entry.
#[derive(Clone, Copy, Debug)]
pub enum MenuItem<'a> {
    /// A command, or with `checked` a check item, reported by its `id`.
    Item {
        /// Reported in [`Event::TrayMenu`].
        id: u32,
        /// The text.
        label: &'a str,
        /// Whether it can be chosen.
        enabled: bool,
        /// A check item's state; `None` for a command.
        checked: Option<bool>,
    },
    /// A divider.
    Separator,
    /// A nested menu.
    Submenu {
        /// The text.
        label: &'a str,
        /// Its entries.
        items: &'a [MenuItem<'a>],
    },
}

/// A tray icon.
#[derive(Clone, Copy, Debug)]
pub struct Tray<'a> {
    /// The icon; desktops scale it to their tray.
    pub icon: Icon<'a>,
    /// The hover text.
    pub tooltip: &'a str,
    /// The menu a secondary click opens.
    pub menu: &'a [MenuItem<'a>],
}

/// A global shortcut.
#[derive(Clone, Copy, Debug)]
pub struct Shortcut<'a> {
    /// Reported in [`Event::Shortcut`].
    pub id: &'a str,
    /// What it does, shown by desktops that let the user confirm or change it.
    pub description: &'a str,
    /// The preferred keys. On Linux the desktop may assign others.
    pub trigger: Trigger,
}

/// The desktop services of one application.
pub struct Desktop(imp::Desktop);

impl Desktop {
    /// Connects to the desktop as `app_id` (a reverse-DNS name such as
    /// `org.example.Editor`) and starts the thread that passes events to
    /// `sink`. Fails if there is no desktop session to talk to, such as a
    /// Linux session without a bus.
    pub fn new(
        app_id: &str,
        sink: impl Fn(Event) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        imp::Desktop::new(app_id, Box::new(sink)).map(Self)
    }

    /// Shows a dialog choosing files or folders to open; the result arrives
    /// as [`Event::Files`] with the returned request id.
    pub fn open_file(&self, dialog: &FileDialog<'_>) -> std::io::Result<u32> {
        self.0.file_dialog(dialog, false)
    }

    /// Shows a dialog choosing where to save; the result arrives as
    /// [`Event::Files`] with one path.
    pub fn save_file(&self, dialog: &FileDialog<'_>) -> std::io::Result<u32> {
        self.0.file_dialog(dialog, true)
    }

    /// Shows a notification and returns its id.
    pub fn notify(&self, notification: &Notification<'_>) -> std::io::Result<u32> {
        self.0.notify(notification)
    }

    /// Shows or updates the tray icon, or with `None` removes it.
    pub fn set_tray(&self, tray: Option<&Tray<'_>>) -> std::io::Result<()> {
        if let Some(tray) = tray {
            let icon = tray.icon;
            let size = (icon.width as usize)
                .checked_mul(icon.height as usize)
                .and_then(|pixels| pixels.checked_mul(4));
            if icon.width == 0 || icon.height == 0 || size != Some(icon.rgba.len()) {
                return Err(std::io::ErrorKind::InvalidInput.into());
            }
        }
        self.0.set_tray(tray)
    }

    /// Replaces the global shortcuts. Linux desktops may ask the user to
    /// confirm them first.
    pub fn bind_shortcuts(&self, shortcuts: &[Shortcut<'_>]) -> std::io::Result<()> {
        self.0.bind_shortcuts(shortcuts)
    }
}
