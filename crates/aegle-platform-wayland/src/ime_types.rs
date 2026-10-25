//! Contract IME values on text-input-v3 version 1.

use aegle_platform::{Error, ImeRequest, Preedit, Rect};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
    ChangeCause, ContentHint, ContentPurpose,
};

/// Content hints for the focused editor.
pub(crate) fn hints(request: &ImeRequest) -> ContentHint {
    if request.multiline {
        ContentHint::Multiline
    } else {
        ContentHint::empty()
    }
}

pub(crate) const PURPOSE: ContentPurpose = ContentPurpose::Normal;

pub(crate) fn cause(request: &ImeRequest) -> ChangeCause {
    if request.input_method {
        ChangeCause::InputMethod
    } else {
        ChangeCause::Other
    }
}

/// Validates a request and returns its cursor rectangle in protocol
/// coordinates: surface-local logical `[x, y, width, height]`, rounded outward.
pub(crate) fn rect(request: &ImeRequest) -> Result<[i32; 4], Error> {
    request.validate()?;
    let Rect { origin, size } = request.cursor_rect;
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

/// A preedit from its protocol string and byte cursor, where `-1, -1` hides it.
pub(crate) fn preedit(text: Option<String>, begin: i32, end: i32) -> Result<Preedit, &'static str> {
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
    Ok(Preedit { text, cursor })
}
