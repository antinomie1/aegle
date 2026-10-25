use crate::{Error, Rect, Result};

/// Largest surrounding excerpt, in UTF-8 bytes, a request may carry.
pub const MAX_SURROUNDING: usize = 4000;

/// Desired input-method state for the editor focused in a window.
///
/// Byte offsets refer to the excerpt, not the complete document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImeRequest {
    /// Committed text around the selection, excluding preedit, at most
    /// [`MAX_SURROUNDING`] bytes without NUL. `None` composes without
    /// surrounding text, for example when the selection alone is too long.
    pub surrounding: Option<String>,
    /// UTF-8 byte offset of the active selection endpoint.
    pub cursor: usize,
    /// UTF-8 byte offset of the fixed selection endpoint.
    pub anchor: usize,
    /// Caret or selection area in window logical coordinates, including scrolling.
    pub cursor_rect: Rect,
    /// Whether the editor accepts line breaks.
    pub multiline: bool,
    /// Whether the change being reported came from the input method rather
    /// than from the keyboard, pointer or application.
    pub input_method: bool,
}

impl ImeRequest {
    /// Checks the excerpt, its offsets and the caret geometry; platforms
    /// call this once before converting units.
    pub fn validate(&self) -> Result<()> {
        if let Some(text) = &self.surrounding
            && (text.len() > MAX_SURROUNDING || text.contains('\0'))
        {
            return Err(Error::InvalidIme(
                "surrounding text exceeds 4000 bytes or contains NUL",
            ));
        }
        let boundaries = match &self.surrounding {
            Some(text) => text.is_char_boundary(self.cursor) && text.is_char_boundary(self.anchor),
            None => self.cursor == 0 && self.anchor == 0,
        };
        if !boundaries {
            return Err(Error::InvalidIme(
                "surrounding offsets must be UTF-8 boundaries",
            ));
        }
        let Rect { origin, size } = self.cursor_rect;
        if ![origin.x, origin.y, size.width, size.height]
            .iter()
            .all(|value| value.is_finite())
            || size.width < 0.0
            || size.height < 0.0
        {
            return Err(Error::InvalidIme(
                "cursor rectangle must be finite with nonnegative size",
            ));
        }
        Ok(())
    }
}

/// A composition focus change or an atomic composition transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    /// Native text-input focus entered the window; resend the editor state.
    Entered,
    /// Focus left; cancel the preedit without committing it again.
    Left,
    /// Apply to the focused editor, the same one keyboard input edits.
    Update(ImeUpdate),
}

/// One input-method edit, applied in field order: delete around the
/// selection (or current preedit), replace it with `commit`, then show
/// `preedit` after the commit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImeUpdate {
    /// UTF-8 bytes to delete before the selection or preedit, excluding it.
    pub delete_before: usize,
    /// UTF-8 bytes to delete after the selection or preedit, excluding it.
    pub delete_after: usize,
    /// Committed replacement; `Some("")` deletes, unlike no commit.
    pub commit: Option<String>,
    /// New preedit, empty when the composition ends.
    pub preedit: Preedit,
}

/// Preedit text and its cursor or highlighted range, in UTF-8 byte offsets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// Temporary text not yet committed to the document.
    pub text: String,
    /// Fixed and active endpoints within `text`; `None` hides the cursor.
    pub cursor: Option<(usize, usize)>,
}
