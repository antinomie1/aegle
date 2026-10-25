use crate::{Error, ImeRequest};

/// Validates a request and returns its caret rectangle in physical client
/// pixels at `scale`, rounded outward, as `[left, top, right, bottom]`.
pub(crate) fn caret(request: &ImeRequest, scale: f32) -> Result<[i32; 4], Error> {
    request.validate()?;
    let r = request.cursor_rect;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(Error::InvalidIme("invalid candidate geometry"));
    }
    let scale = f64::from(scale);
    let edges = [
        (f64::from(r.origin.x) * scale).floor(),
        (f64::from(r.origin.y) * scale).floor(),
        ((f64::from(r.origin.x) + f64::from(r.size.width)) * scale).ceil(),
        ((f64::from(r.origin.y) + f64::from(r.size.height)) * scale).ceil(),
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
