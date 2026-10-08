//! Reading and replacing the text of text-bearing controls.

use aegle_core::{Dirty, NodeId};

use crate::{Result, State, UiError};

impl State {
    /// Replaces the text of a label, button, toggle or editor, invalidating its
    /// layout, scene and semantics. An editor also restarts a focused IME
    /// session. Typed handles of text-bearing controls wrap it; other controls
    /// return [`UiError::WrongKind`].
    pub fn set_text(&mut self, id: NodeId, text: &str) -> Result {
        let style = self.text_style(id);
        let control = &mut self.tree.get_mut(id).unwrap().context.control;
        if let Some(paragraph) = control.paragraph_mut() {
            self.fonts.borrow_mut().update(paragraph, text, &style)?;
        } else if let Some(field) = control.editor_mut() {
            self.fonts
                .borrow_mut()
                .edit(field.editor_mut())
                .set_text(text)?;
            if self.focus.current(&self.tree) == Some(id) {
                self.ime_dirty = true;
                self.ime_reset = true;
                self.input_method = false;
            }
        } else {
            return Err(UiError::WrongKind.into());
        }
        self.tree.mark_dirty(id, Dirty::ALL)?;
        Ok(())
    }

    /// Copies the displayed text, or an editor's committed text.
    pub fn text(&self, id: NodeId) -> Result<String> {
        let control = &self.tree.get(id).unwrap().context.control;
        if let Some(paragraph) = control.paragraph() {
            Ok(paragraph.text().to_owned())
        } else if let Some(field) = control.editor() {
            Ok(field.editor().text().to_string())
        } else {
            Err(UiError::WrongKind.into())
        }
    }
}
