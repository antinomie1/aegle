use crate::Error;
use aegle_types::Rect;

/// Native IMM composition state. No surrounding-text support is claimed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImeRequest {
    /// Logical client coordinates of the active editor's caret or selection.
    pub cursor_rect: Rect,
}
impl ImeRequest {
    /// Validates finite, nonnegative candidate geometry within native coordinates.
    pub fn validate(&self, scale: f32) -> Result<[i32; 4], Error> {
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
    /// Keyboard focus entered a window with IMM enabled.
    Entered,
    /// Composition ended or focus left; cancel any remaining preedit.
    Left,
    /// Apply to the focused retained editor.
    Update(ImeUpdate),
}

/// IMM result and replacement preedit, applied in that order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeUpdate {
    /// Committed composition result; `Some("")` differs from no result.
    pub commit: Option<String>,
    /// New preedit, empty when the composition ends.
    pub preedit: Preedit,
}

/// Validated UTF-8 preedit and a UTF-8 byte cursor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Preedit {
    /// Temporary text.
    pub text: String,
    /// Collapsed cursor or selection endpoints; None hides the cursor.
    pub cursor: Option<(usize, usize)>,
}

/// Converts a native UTF-16 cursor to a UTF-8 byte boundary, rejecting surrogate splits.
/// Useful for IMM/TSF adapters sharing the same UTF-8 editor contract.
pub fn utf16_cursor(text: &str, position: usize) -> Result<usize, Error> {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units == position {
            return Ok(byte);
        }
        units += ch.len_utf16();
    }
    if units == position {
        Ok(text.len())
    } else {
        Err(Error::InvalidIme(
            "cursor splits UTF-16 scalar or exceeds preedit",
        ))
    }
}
