//! Validated UTF-8 and surface-local geometry for text-input-v3 version 1.

use aegle_types::Rect;
pub use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ChangeCause as ImeCause, ContentHint as ImeHints, ContentPurpose as ImePurpose,
};

use crate::Error;

/// Desired input-method state for the editable control focused in a window.
///
/// Supply at most 4000 UTF-8 bytes around the selection, excluding preedit text.
/// Both byte offsets refer to this excerpt, rather than the complete document.
/// Changing the focused control in the same window requires disabling its old
/// session with `None` before configuring the new control.
/// Disabling is immediate, including while ordinary state updates await a
/// matching compositor acknowledgement; canceled-session batches are discarded.
#[derive(Clone, Debug, PartialEq)]
pub struct ImeRequest {
    /// Surrounding committed text, excluding the preedit and containing no NUL.
    pub surrounding: String,
    /// UTF-8 byte offset of the active selection endpoint.
    pub cursor: usize,
    /// UTF-8 byte offset of the fixed selection endpoint.
    pub anchor: usize,
    /// Cursor area in surface-local logical coordinates, including scroll offsets.
    pub cursor_rect: Rect,
    /// Version 1 content hints; unsupported version 2 bits are rejected.
    pub hints: ImeHints,
    /// The intended content of the focused control.
    pub purpose: ImePurpose,
    /// Use `Other` for keyboard, pointer or application changes, and
    /// `InputMethod` for changes caused by an [`ImeEvent::Update`].
    pub cause: ImeCause,
}

impl Default for ImeRequest {
    fn default() -> Self {
        Self {
            surrounding: String::new(),
            cursor: 0,
            anchor: 0,
            cursor_rect: Rect::default(),
            hints: ImeHints::empty(),
            purpose: ImePurpose::Normal,
            cause: ImeCause::Other,
        }
    }
}

impl ImeRequest {
    pub(crate) fn validate(&self) -> Result<[i32; 4], Error> {
        if self.surrounding.len() > 4000 || self.surrounding.contains('\0') {
            return Err(Error::InvalidIme(
                "surrounding text exceeds 4000 bytes or contains NUL",
            ));
        }
        if !self.surrounding.is_char_boundary(self.cursor)
            || !self.surrounding.is_char_boundary(self.anchor)
        {
            return Err(Error::InvalidIme(
                "surrounding offsets must be UTF-8 boundaries",
            ));
        }
        if self.hints.bits() & !0x3ff != 0 {
            return Err(Error::InvalidIme(
                "content hints require text-input-v3 version 2",
            ));
        }
        let Rect { origin, size } = self.cursor_rect;
        if !origin.x.is_finite()
            || !origin.y.is_finite()
            || !size.width.is_finite()
            || !size.height.is_finite()
            || size.width < 0.0
            || size.height < 0.0
        {
            return Err(Error::InvalidIme(
                "cursor rectangle must be finite with nonnegative size",
            ));
        }
        // Compute edges in f64: f32 cannot distinguish i32::MAX from 2^31.
        let x = f64::from(origin.x).floor();
        let y = f64::from(origin.y).floor();
        let right = (f64::from(origin.x) + f64::from(size.width)).ceil();
        let bottom = (f64::from(origin.y) + f64::from(size.height)).ceil();
        let values = [x, y, right - x, bottom - y];
        if values
            .iter()
            .any(|value| *value < f64::from(i32::MIN) || *value > f64::from(i32::MAX))
        {
            return Err(Error::InvalidIme(
                "cursor rectangle exceeds protocol coordinates",
            ));
        }
        Ok(values.map(|value| value as i32))
    }
}

/// A text-input focus change or an atomic composition transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    /// The seat's text-input focus entered this window.
    Entered,
    /// Focus left; cancel the control's preedit without committing it again.
    Left,
    /// Apply the transaction to the same retained editor used for keyboard input.
    Update(ImeUpdate),
}

/// Pending operations committed by a text-input-v3 `done` event.
///
/// Apply these in protocol order: remove the old preedit, delete surrounding
/// bytes excluding the selection, insert the commit, determine surrounding
/// state, then insert the new preedit. Validate deletion byte boundaries against
/// the editor before applying. A missing preedit always clears the previous one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeUpdate {
    /// Number of client commits acknowledged by the compositor, wrapping at u32.
    pub serial: u32,
    /// Whether this acknowledges the most recent client commit at dispatch.
    ///
    /// Stale transactions in this enabled session must still be applied. Send
    /// resulting state only after a current transaction with `ImeCause::InputMethod`; always
    /// respond with the final editor state after processing queued transactions.
    pub current: bool,
    /// Text to commit; `Some("")` is distinct from no commit event.
    pub commit: Option<String>,
    /// New composition, defaulting to empty for every transaction.
    pub preedit: Preedit,
    /// UTF-8 bytes to delete before the cursor, excluding selected text.
    pub delete_before: u32,
    /// UTF-8 bytes to delete after the cursor, excluding selected text.
    pub delete_after: u32,
}

/// Preedit text and its cursor or highlighted range, in UTF-8 byte offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    /// Temporary text that has not been committed to the document.
    pub text: String,
    /// Directed cursor range; `None` hides the preedit cursor.
    pub cursor: Option<(usize, usize)>,
}

impl Default for Preedit {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: Some((0, 0)),
        }
    }
}

impl Preedit {
    pub(crate) fn from_protocol(
        text: Option<String>,
        begin: i32,
        end: i32,
    ) -> Result<Self, &'static str> {
        let text = text.unwrap_or_default();
        let cursor = if begin == -1 && end == -1 {
            None
        } else if begin >= 0
            && end >= 0
            && text.is_char_boundary(begin as usize)
            && text.is_char_boundary(end as usize)
        {
            Some((begin as usize, end as usize))
        } else {
            return Err("preedit cursor must be hidden or contain valid UTF-8 offsets");
        };
        Ok(Self { text, cursor })
    }
}
