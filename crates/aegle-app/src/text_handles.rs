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
    fn set_text(&self, text: &str) -> Result {
        self.change(|state, id| {
            let style = state.text_style(id);
            match &mut state.tree.get_mut(id).unwrap().context.content {
                Content::Label(p) | Content::Button(_, p) => {
                    state.fonts.borrow_mut().update(p, text, &style)?
                }
                Content::Field(field) => {
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
                }
                _ => return Err(UiError::WrongKind.into()),
            }
            state.tree.mark_dirty(id, Dirty::ALL)?;
            Ok(())
        })
    }
    fn text(&self) -> Result<String> {
        self.change(|state, id| {
            Ok(match &state.tree.get(id).unwrap().context.content {
                Content::Label(p) | Content::Button(_, p) => p.text().to_owned(),
                Content::Field(field) => field.editor().text().to_string(),
                _ => return Err(UiError::WrongKind.into()),
            })
        })
    }
}
