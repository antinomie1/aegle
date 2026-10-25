//! A path field with a button that opens the native file dialog.

use std::path::Path;

use aegle_theme::ControlKind;
use aegle_types::DragData;
use aegle_ui::{Align, Direction};
use aegle_ui::{
    Container, Control, DropEvent, FileDialog, FileMode, HandlerResult, Node, Subscription,
    container_style, control::Frame, require,
};

use crate::{Button, TextField};

/// The control at a [`FilePicker`] root: the dialog its button opens. The
/// field and the button are its first and second children.
pub struct FilePickerControl {
    dialog: FileDialog,
}

impl Control for FilePickerControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn frame(&self) -> Frame {
        Frame {
            background: false,
            border: false,
        }
    }
}

aegle_ui::handle! {
    /// A path field followed by a button that opens the host's native file
    /// dialog; a file dropped on it is chosen too. The field stays editable,
    /// so a path can also be typed or pasted. Paths that are not valid UTF-8
    /// are shown lossily.
    pub FilePicker(FilePickerControl)
}

impl FilePicker {
    /// The path in the field.
    pub fn path(&self) -> String {
        self.field().text()
    }
    /// Replaces the path without calling the change handlers.
    pub fn set_path(&self, path: &str) {
        self.field().set_text(path)
    }
    /// The path field, for its style or read-only state.
    pub fn field(&self) -> TextField {
        TextField(self.part(0))
    }
    /// The button that opens the dialog, for its text or style.
    pub fn button(&self) -> Button {
        Button(self.part(1))
    }
    /// The dialog the button opens.
    pub fn dialog(&self) -> FileDialog {
        self.read(|picker| picker.dialog.clone())
    }
    /// Sets the dialog the button opens. A picker holds one path, so the
    /// mode must not be [`FileMode::OpenMany`].
    pub fn set_dialog(&self, dialog: FileDialog) {
        require(dialog.mode != FileMode::OpenMany);
        self.update(|picker| picker.dialog = dialog)
    }
    /// Asks the host to show the dialog, starting in the current path's
    /// folder and, when saving, suggesting its file name. A chosen path
    /// replaces the field and calls the change handlers.
    pub fn browse(&self) {
        let mut dialog = self.dialog();
        let current = self.path();
        let current = Path::new(&current);
        if dialog.folder.is_none() {
            let folder = if current.is_dir() {
                Some(current)
            } else {
                current.parent()
            };
            dialog.folder = folder.filter(|f| f.is_dir()).map(Path::to_path_buf);
        }
        if dialog.mode == FileMode::Save && dialog.name.is_empty() {
            let name = current.file_name().map(|name| name.to_string_lossy());
            dialog.name = name.unwrap_or_default().into_owned();
        }
        self.choose_files(dialog, |node, paths| {
            if let Some(path) = paths.and_then(|paths| paths.into_iter().next()) {
                FilePicker(node).choose(&path);
            }
            Ok(())
        })
    }
    /// Adds a handler called after the user chose a path in the dialog,
    /// dropped a file or edited the field.
    pub fn on_change<R: HandlerResult>(
        &self,
        mut callback: impl FnMut(FilePicker) -> R + 'static,
    ) -> Subscription {
        self.on_action(move |node| callback(FilePicker(node)).into_result())
    }
    fn choose(&self, path: &Path) {
        self.set_path(&path.to_string_lossy());
        self.changed();
    }
    fn changed(&self) {
        self.change(|state, id| {
            state.queue_action(id);
            Ok(())
        })
    }
    fn part(&self, index: usize) -> Node {
        let id = self.change(|state, id| Ok(state.tree.child(id, index)?.unwrap()));
        Node {
            state: self.state.clone(),
            id,
        }
    }
}

pub(crate) fn file_picker(container: &Container, mode: FileMode) -> FilePicker {
    let root = Container(crate::add(container, |_, theme| {
        let mut style = container_style(theme, false);
        style
            .set_direction(Direction::Row)
            .set_align_items(Some(Align::Center));
        let dialog = FileDialog {
            mode,
            ..Default::default()
        };
        Ok((
            Box::new(FilePickerControl { dialog }) as Box<dyn Control>,
            style,
        ))
    }));
    require(mode != FileMode::OpenMany);
    let picker = FilePicker(root.0.clone());
    let field = crate::field::create(&root, "", false);
    field.set_grow(1.0);
    let button = crate::button::create(&root, "Browse…");
    let edited = picker.clone();
    field.on_change(move |_| edited.changed());
    let browsing = picker.clone();
    button.on_click(move |_| browsing.browse());
    let dropped = picker.clone();
    picker.on_drop(move |_, event| {
        if let DropEvent::Drop {
            data: DragData::Files(paths),
            ..
        } = event
            && let Some(path) = paths.first()
        {
            dropped.choose(path);
        }
    });
    picker
}
