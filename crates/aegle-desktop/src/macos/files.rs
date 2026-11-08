//! File dialogs as modeless `NSOpenPanel` and `NSSavePanel` windows.
//!
//! `beginWithCompletionHandler:` returns at once and calls back when the user
//! closes the panel, so the application's event loop keeps running while it
//! is open. AppKit panels have no filter menu: every filter's extensions are
//! allowed together, and a filter that is not `*.extension`, such as `*`,
//! allows every file.

use std::{path::PathBuf, sync::Arc};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSSavePanel};
use objc2_foundation::{MainThreadMarker, NSArray, NSString, NSURL};

use super::Sink;
use crate::{Event, FileDialog};

/// The extensions `filters` allow, or `None` for every file.
fn extensions(filters: &[(&str, &[&str])]) -> Option<Vec<String>> {
    let mut allowed = Vec::new();
    for pattern in filters.iter().flat_map(|(_, patterns)| patterns.iter()) {
        // The last dot's extension: AppKit matches `tar.gz` files as `gz`.
        let extension = pattern.strip_prefix("*.")?.rsplit('.').next()?;
        if extension.is_empty() || extension.contains(['*', '?', '[']) {
            return None;
        }
        allowed.push(extension.to_owned());
    }
    (!allowed.is_empty()).then_some(allowed)
}

fn url_path(url: &NSURL) -> Option<PathBuf> {
    url.path().map(|path| PathBuf::from(path.to_string()))
}

/// Shows the panel for `dialog`; [`Event::Files`] with `request` follows when
/// it closes.
pub(super) fn show(
    mtm: MainThreadMarker,
    sink: &Arc<Sink>,
    request: u32,
    dialog: &FileDialog<'_>,
    save: bool,
) {
    let (panel, open): (Retained<NSSavePanel>, _) = if save {
        (NSSavePanel::savePanel(mtm), None)
    } else {
        let open = NSOpenPanel::openPanel(mtm);
        open.setAllowsMultipleSelection(dialog.multiple);
        open.setCanChooseDirectories(dialog.directory);
        open.setCanChooseFiles(!dialog.directory);
        (Retained::into_super(open.clone()), Some(open))
    };
    if !dialog.title.is_empty() {
        let title = NSString::from_str(dialog.title);
        panel.setTitle(Some(&title));
        // Modern panels show the message, not the title.
        panel.setMessage(Some(&title));
    }
    if save && !dialog.name.is_empty() {
        panel.setNameFieldStringValue(&NSString::from_str(dialog.name));
    }
    if let Some(folder) = dialog.folder {
        let path = NSString::from_str(&folder.to_string_lossy());
        panel.setDirectoryURL(Some(&NSURL::fileURLWithPath_isDirectory(&path, true)));
    }
    if !dialog.directory {
        let allowed = extensions(dialog.filters).map(|allowed| {
            let strings: Vec<_> = allowed.iter().map(|ext| NSString::from_str(ext)).collect();
            NSArray::from_retained_slice(&strings)
        });
        #[allow(deprecated)] // The replacement needs UTTypes for each extension.
        panel.setAllowedFileTypes(allowed.as_deref());
    }
    let sink = sink.clone();
    let shown = panel.clone();
    let handler = RcBlock::new(move |response| {
        let paths = (response == NSModalResponseOK).then(|| match &open {
            Some(open) => open
                .URLs()
                .iter()
                .filter_map(|url| url_path(&url))
                .collect(),
            None => shown
                .URL()
                .as_deref()
                .and_then(url_path)
                .into_iter()
                .collect(),
        });
        sink(Event::Files { request, paths });
    });
    panel.beginWithCompletionHandler(&handler);
}
