//! Native file dialogs that controls ask their host to show, answered with
//! the chosen paths like the clipboard: the host takes requests after
//! callbacks and refresh, shows them, and later delivers the result.

use std::{collections::HashMap, path::PathBuf};

use aegle_core::NodeId;

use crate::{Node, Result, State, Ui};

/// What a native file dialog chooses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileMode {
    /// One existing file.
    #[default]
    Open,
    /// One or more existing files.
    OpenMany,
    /// Where to save a file.
    Save,
    /// One folder.
    Folder,
}

/// A native file dialog a control asked its host to show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileDialog {
    /// The window title.
    pub title: String,
    /// Named filters with their glob patterns, such as
    /// `("Images", ["*.png", "*.jpg"])`; the first one is selected.
    pub filters: Vec<(String, Vec<String>)>,
    /// What it chooses.
    pub mode: FileMode,
    /// Saving: the suggested file name.
    pub name: String,
    /// The folder it starts in; `None` leaves it to the desktop.
    pub folder: Option<PathBuf>,
}

/// Names one dialog between [`Ui::take_file_dialogs`] and [`Ui::files_chosen`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileRequest(u64);

type Chosen = Box<dyn FnOnce(Node, Option<Vec<PathBuf>>) -> Result>;

/// Dialogs waiting for the host and requesters waiting for an answer.
#[derive(Default)]
pub(crate) struct Files {
    next: u64,
    waiting: Vec<(FileRequest, FileDialog)>,
    open: HashMap<FileRequest, (NodeId, Chosen)>,
}

impl State {
    /// Asks the host to show `dialog` for `id`. `chosen` runs once, outside
    /// the UI borrow, with the chosen paths or `None` when the user or the
    /// host cancelled it; it does not run if `id` was removed meanwhile.
    pub fn choose_files(
        &mut self,
        id: NodeId,
        dialog: FileDialog,
        chosen: impl FnOnce(Node, Option<Vec<PathBuf>>) -> Result + 'static,
    ) {
        let request = FileRequest(self.files.next);
        self.files.next += 1;
        self.files.waiting.push((request, dialog));
        self.files.open.insert(request, (id, Box::new(chosen)));
    }
}

impl Node {
    /// Asks the host to show a native file dialog; see [`State::choose_files`].
    pub fn choose_files(
        &self,
        dialog: FileDialog,
        chosen: impl FnOnce(Node, Option<Vec<PathBuf>>) -> Result + 'static,
    ) {
        self.change(|state, id| {
            state.choose_files(id, dialog, chosen);
            Ok(())
        })
    }
}

impl Ui {
    /// Dialogs controls asked for since the last call. The host shows each one
    /// and answers it with [`Self::files_chosen`], with `None` if it cannot.
    pub fn take_file_dialogs(&self) -> Vec<(FileRequest, FileDialog)> {
        std::mem::take(&mut self.write().files.waiting)
    }

    /// Delivers the answer to `request`, running its requester's callback
    /// unless the control was removed; returns the callback's result. Call it
    /// outside callbacks, like [`Self::dispatch_callbacks`].
    pub fn files_chosen(&self, request: FileRequest, paths: Option<Vec<PathBuf>>) -> Result {
        let (id, chosen) = {
            let mut state = self.write();
            let Some((id, chosen)) = state.files.open.remove(&request) else {
                return Ok(());
            };
            if state.tree.get(id).is_none() {
                return Ok(());
            }
            (id, chosen)
        };
        let node = Node {
            id,
            state: std::rc::Rc::downgrade(&self.state),
        };
        chosen(node, paths)
    }
}
