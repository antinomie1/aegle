use crate::{Button, Label, Node, Result, TextField, UiError, state::Content};
use aegle_core::Dirty;
use aegle_text::Selection;

impl Label {
    /// Replaces text and invalidates its shared layout, scene and semantic state.
    pub fn set_text(&self, text: &str) -> Result {
        self.0.set_text(text)
    }
    /// Copies the current display text.
    pub fn text(&self) -> Result<String> {
        self.0.text()
    }
}
impl Button {
    /// Replaces the button label.
    pub fn set_text(&self, text: &str) -> Result {
        self.0.set_text(text)
    }
    /// Queues semantic activation using the normal enabled/visible behavior.
    pub fn activate(&self) -> Result {
        self.change(|state, id| state.dispatch(id, aegle_controls::Input::Activate))
    }
}
impl TextField {
    /// Replaces the committed text, clears history and explicitly ends native preedit.
    pub fn set_text(&self, text: &str) -> Result {
        self.0.set_text(text)
    }
    /// Copies committed text, never substituting transient preedit.
    pub fn text(&self) -> Result<String> {
        self.0.text()
    }
    /// Allows selection but rejects user edits when true.
    pub fn set_read_only(&self, read_only: bool) -> Result {
        self.change(|state, id| {
            let Content::Field(field) = &mut state.tree.get_mut(id).unwrap().context.content else {
                unreachable!()
            };
            state
                .fonts
                .borrow_mut()
                .edit(field.editor_mut())
                .set_read_only(read_only);
            if state.focus.current(&state.tree) == Some(id) {
                state.ime_dirty = true;
                state.ime_reset = true;
                state.input_method = false;
            }
            Ok(())
        })
    }
    /// Changes the committed UTF-8 selection; active preedit must first be cancelled.
    pub fn select(&self, selection: Selection) -> Result {
        self.change(|state, id| {
            let Content::Field(field) = &mut state.tree.get_mut(id).unwrap().context.content else {
                unreachable!()
            };
            state
                .fonts
                .borrow_mut()
                .edit(field.editor_mut())
                .select(selection)?;
            if state.focus.current(&state.tree) == Some(id) {
                state.ime_dirty = true;
                state.ime_reset = true;
                state.input_method = false;
            }
            Ok(())
        })
    }
}
impl Node {
    pub(crate) fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| {
            let style = state.text_style(id);
            let content = &mut state.tree.get_mut(id).unwrap().context.content;
            if let Some(paragraph) = content.paragraph_mut() {
                state.fonts.borrow_mut().update(paragraph, text, &style)?;
            } else if let Content::Field(field) = content {
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
    pub(crate) fn text(&self) -> Result<String> {
        self.change(|state, id| {
            let content = &state.tree.get(id).unwrap().context.content;
            if let Some(paragraph) = content.paragraph() {
                Ok(paragraph.text().to_owned())
            } else if let Content::Field(field) = content {
                Ok(field.editor().text().to_string())
            } else {
                Err(UiError::WrongKind.into())
            }
        })
    }
}
