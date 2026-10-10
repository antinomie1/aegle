use crate::Error;
use aegle_types::Rect;

/// Largest surrounding excerpt, in UTF-8 bytes, a request may carry.
pub const MAX_SURROUNDING: usize = 4000;

/// Desired input-method state for the editable control focused in a window.
///
/// The excerpt becomes the TSF document that input methods read for
/// prediction and reconversion; both byte offsets refer to it rather than to
/// the complete text. Changing the focused control in the same window requires
/// disabling its old session with `None` before configuring the new control.
#[derive(Clone, Debug, PartialEq)]
pub struct ImeRequest {
    /// Committed text around the selection, excluding preedit, at most
    /// [`MAX_SURROUNDING`] bytes. `None` enables composition with an empty
    /// document, for example when the selection alone exceeds the limit.
    pub surrounding: Option<String>,
    /// UTF-8 byte offset of the active selection endpoint.
    pub cursor: usize,
    /// UTF-8 byte offset of the fixed selection endpoint.
    pub anchor: usize,
    /// Logical client coordinates of the active editor's caret or selection.
    pub cursor_rect: Rect,
}
impl Default for ImeRequest {
    fn default() -> Self {
        Self {
            surrounding: Some(String::new()),
            cursor: 0,
            anchor: 0,
            cursor_rect: Rect::default(),
        }
    }
}
impl ImeRequest {
    /// Validates the excerpt and its offsets, then returns the caret rectangle
    /// in physical client pixels, rounded outward, as `[left, top, right, bottom]`.
    pub fn validate(&self, scale: f32) -> Result<[i32; 4], Error> {
        if self
            .surrounding
            .as_ref()
            .is_some_and(|text| text.len() > MAX_SURROUNDING)
        {
            return Err(Error::InvalidIme("surrounding text exceeds 4000 bytes"));
        }
        if self
            .surrounding
            .as_ref()
            .map_or(self.cursor != 0 || self.anchor != 0, |text| {
                !text.is_char_boundary(self.cursor) || !text.is_char_boundary(self.anchor)
            })
        {
            return Err(Error::InvalidIme(
                "surrounding offsets must be UTF-8 boundaries",
            ));
        }
        let r = self.cursor_rect;
        if !scale.is_finite() || scale <= 0.0 || r.size.width < 0.0 || r.size.height < 0.0 {
            return Err(Error::InvalidIme("invalid candidate geometry"));
        }
        let scale = f64::from(scale);
        let edges = [
            f64::from(r.origin.x) * scale,
            f64::from(r.origin.y) * scale,
            (f64::from(r.origin.x) + f64::from(r.size.width)) * scale,
            (f64::from(r.origin.y) + f64::from(r.size.height)) * scale,
        ];
        let edges = [
            edges[0].floor(),
            edges[1].floor(),
            edges[2].ceil(),
            edges[3].ceil(),
        ];
        if edges
            .iter()
            .any(|v| !v.is_finite() || *v < i32::MIN as f64 || *v > i32::MAX as f64)
        {
            return Err(Error::InvalidIme(
                "candidate rectangle exceeds native coordinates",
            ));
        }
        Ok(edges.map(|v| v as i32))
    }
}

/// Native focus or atomic composition update.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    /// Keyboard focus entered a window with an enabled session.
    Entered,
    /// Focus left; cancel any remaining preedit.
    Left,
    /// Apply to the focused retained editor.
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

/// Validated UTF-8 preedit and a UTF-8 byte cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// Temporary text.
    pub text: String,
    /// Fixed and active endpoints within `text`; None hides the cursor.
    pub cursor: Option<(usize, usize)>,
}
