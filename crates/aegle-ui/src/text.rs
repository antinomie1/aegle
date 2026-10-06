//! Reading and replacing the text of text-bearing controls.

use aegle_core::Dirty;

use crate::{Node, Result, UiError};

impl Node {
    /// Replaces the text of a label, button, toggle or editor, invalidating its
    /// layout, scene and semantics. An editor also restarts a focused IME session.
    /// Other controls return [`UiError::WrongKind`].
    pub fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| {
            let style = state.text_style(id);
            let control = &mut state.tree.get_mut(id).unwrap().context.control;
            if let Some(paragraph) = control.paragraph_mut() {
                state.fonts.borrow_mut().update(paragraph, text, &style)?;
            } else if let Some(field) = control.editor_mut() {
                state
                    .fonts
                    .borrow_mut()
                    .edit(field.editor_mut())
                    .set_text(text)?;
                if state.focus.current(&state.tree) == Some(id) {
                    state.ime_dirty = true;
                    state.ime_reset = true;
                    state.input_method = false;
                }
            } else {
                return Err(UiError::WrongKind.into());
            }
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })
    }

    /// Copies the displayed text, or an editor's committed text.
    pub fn text(&self) -> Result<String> {
        self.change(|state, id| {
            let control = &state.tree.get(id).unwrap().context.control;
            if let Some(paragraph) = control.paragraph() {
                Ok(paragraph.text().to_owned())
            } else if let Some(field) = control.editor() {
                Ok(field.editor().text().to_string())
            } else {
                Err(UiError::WrongKind.into())
            }
        })
    }
}
