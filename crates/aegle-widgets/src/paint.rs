//! Check boxes, radio buttons, switches, sliders, progress bars, chevrons and
//! the accent bar that marks a chosen entry.
//!
//! Shapes follow the default visual system: markers that hold a state or a
//! value (radio rings, switch tracks and thumbs, range tracks, slider thumbs
//! and the chosen-entry bar) are round whatever the corner radius; check
//! boxes take the appearance radius like other rectangles.
use aegle_scene::{Affine, Color, Rect, RoundedRect, SceneBuilder};
use aegle_theme::Appearance;
use aegle_types::Size;
use aegle_ui::OrFail;

/// Width of the dropdown chevron, shared with layout.
pub const CHEVRON: f32 = 10.0;
/// Diameter of a slider thumb, shared with pointer-to-value mapping.
pub const THUMB: f32 = 20.0;
/// Width of a check box or radio marker.
const BOX: f32 = 20.0;
/// Width and height of a switch track.
const TRACK: (f32, f32) = (40.0, 20.0);
/// Thickness of range tracks and fills.
const RAIL: f32 = 4.0;

/// How a two-state control draws its marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Square box with a check or, when mixed, a bar.
    Check,
    /// Pill track with a sliding thumb.
    Switch,
    /// Round marker with a dot; exclusive among siblings by convention.
    Radio,
}

impl Mark {
    /// The marker's width, which layout reserves before the label.
    pub fn width(self) -> f32 {
        match self {
            Self::Switch => TRACK.0,
            Self::Check | Self::Radio => BOX,
        }
    }
}

/// Geometry and state of a toggle's marker and optional label.
#[derive(Clone, Copy, Debug)]
pub struct ToggleSpec {
    /// Control bounds.
    pub size: Size,
    /// Inset from the control edge to the marker.
    pub padding: f32,
    /// Space between the marker and its label.
    pub gap: f32,
    /// Marker shape.
    pub mark: Mark,
    /// Whether the control is on.
    pub checked: bool,
    /// A check box shown as partially checked.
    pub mixed: bool,
    /// Where a switch's thumb is, from off (0) to on (1); a host animates it
    /// between the two. Other marks ignore it.
    pub travel: f32,
    /// Size of the label, or `None` when there is no label to paint.
    pub label: Option<Size>,
    /// Laid out right to left: the marker sits at the right edge with the label
    /// to its left, and a switch's thumb moves leftward when on.
    pub rtl: bool,
}

/// Paints a toggle. The appearance's background and border are the marker's;
/// the indicator draws a mixed bar and on-indicator the check, dot or on
/// thumb, while an off thumb takes the border color. `label` receives the
/// builder, already translated to the label origin, and the foreground color;
/// it runs only when the spec has a label.
pub fn toggle(
    builder: &mut SceneBuilder,
    spec: &ToggleSpec,
    appearance: Appearance,
    label: impl FnOnce(&mut SceneBuilder, Color),
) {
    let ToggleSpec {
        size,
        padding,
        gap,
        mark,
        checked,
        mixed,
        travel,
        label: label_size,
        rtl,
    } = *spec;
    builder.push_clip(RoundedRect::new(
        Rect::new(0.0, 0.0, size.width, size.height),
        0.0,
    ));
    let x = padding.min(size.width * 0.5);
    let y_padding = padding.min(size.height * 0.5);
    let (width, height) = match mark {
        Mark::Switch => TRACK,
        Mark::Check | Mark::Radio => (BOX, BOX),
    };
    let scale = ((size.width - 2.0 * x) / width)
        .min((size.height - 2.0 * y_padding) / height)
        .min(1.0);
    let (width, height) = (width * scale, height * scale);
    let y = (size.height - height) * 0.5;
    let x = if rtl { size.width - x - width } else { x };
    let marker = Rect::new(x, y, width, height);
    let radius = match mark {
        Mark::Check => appearance.radius,
        Mark::Switch | Mark::Radio => height * 0.5,
    };
    let frame = Appearance {
        radius,
        ..appearance
    };
    fill(builder, marker, radius, appearance.background);
    border(builder, marker, frame);
    match mark {
        Mark::Radio if checked => {
            let diameter = width * 0.4;
            let inset = (width - diameter) * 0.5;
            let dot = Rect::new(x + inset, y + inset, diameter, diameter);
            fill(builder, dot, diameter * 0.5, appearance.on_indicator);
        }
        Mark::Check if mixed => {
            let thickness = 2.0 * scale;
            let bar = Rect::new(
                x + width * 0.25,
                y + (height - thickness) * 0.5,
                width * 0.5,
                thickness,
            );
            fill(builder, bar, thickness * 0.5, appearance.indicator);
        }
        Mark::Check if checked => check_mark(builder, x, y, width, appearance.on_indicator),
        Mark::Switch => {
            let diameter = height * 0.6;
            let inset = (height - diameter) * 0.5;
            let travel = travel.clamp(0.0, 1.0);
            let span = width - 2.0 * inset - diameter;
            let along = if rtl { 1.0 - travel } else { travel };
            let thumb = Rect::new(x + inset + span * along, y + inset, diameter, diameter);
            let color = appearance.border_color.mix(appearance.on_indicator, travel);
            fill(builder, thumb, diameter * 0.5, color);
        }
        _ => {}
    }
    if let Some(text) = label_size {
        let left = if rtl {
            x - gap - text.width
        } else {
            x + width + gap
        };
        builder.push_transform(Affine::translation(left, (size.height - text.height) * 0.5));
        label(builder, appearance.foreground);
        builder.pop();
    }
    builder.pop();
}

/// Start and length of a slider's track, shared with pointer-to-value mapping.
/// The thumb stays inside tiny controls.
pub fn slider_track(size: Size, padding: f32) -> (f32, f32) {
    let diameter = THUMB.min(size.width).min(size.height);
    let start = (padding + diameter * 0.5).min(size.width * 0.5);
    (start, (size.width - 2.0 * start).max(0.0))
}

/// Paints a horizontal progress bar or, with a `thumb`, slider, whose
/// indicator covers the `filled` span of fractions, `0.0..=1.0`. The track is
/// the appearance background; a slider's thumb, at `filled[1]`, is an
/// on-indicator disc ringed in the border color around an indicator dot of
/// diameter `thumb`.
pub fn range(
    builder: &mut SceneBuilder,
    size: Size,
    padding: f32,
    filled: [f64; 2],
    thumb: Option<f32>,
    appearance: Appearance,
) {
    builder.push_clip(RoundedRect::new(
        Rect::new(0.0, 0.0, size.width, size.height),
        0.0,
    ));
    let (start, length) = if thumb.is_some() {
        slider_track(size, padding)
    } else {
        let start = padding.min(size.width * 0.5);
        (start, (size.width - 2.0 * start).max(0.0))
    };
    let height = RAIL.min(size.height);
    let y = (size.height - height) * 0.5;
    let track = Rect::new(start, y, length, height);
    fill(builder, track, height * 0.5, appearance.background);
    let [from, to] = filled.map(|f| length * f.clamp(0.0, 1.0) as f32);
    fill(
        builder,
        Rect::new(start + from, y, (to - from).max(0.0), height),
        height * 0.5,
        appearance.indicator,
    );
    border(
        builder,
        track,
        Appearance {
            radius: height * 0.5,
            ..appearance
        },
    );
    if let Some(dot) = thumb {
        let diameter = THUMB.min(size.width).min(size.height);
        let disc = Rect::new(
            start + to - diameter * 0.5,
            (size.height - diameter) * 0.5,
            diameter,
            diameter,
        );
        fill(builder, disc, diameter * 0.5, appearance.on_indicator);
        let ring = 1.0_f32.min(diameter * 0.5);
        stroke(builder, disc, diameter * 0.5, ring, appearance.border_color);
        let dot = dot.min(diameter);
        let inset = (diameter - dot) * 0.5;
        let center = Rect::new(disc.origin.x + inset, disc.origin.y + inset, dot, dot);
        fill(builder, center, dot * 0.5, appearance.indicator);
    }
    builder.pop();
}

/// The short rounded bar that marks the chosen entry of a list, menu or tree,
/// at the leading edge of a `size` row (the right edge right to left).
pub fn chosen_bar(builder: &mut SceneBuilder, size: Size, rtl: bool, color: Color) {
    let (width, height) = (3.0_f32.min(size.width), 16.0_f32.min(size.height * 0.6));
    let x = if rtl { size.width - width } else { 0.0 };
    let bar = Rect::new(x, (size.height - height) * 0.5, width, height);
    fill(builder, bar, width * 0.5, color);
}

fn fill(builder: &mut SceneBuilder, rect: Rect, radius: f32, color: Color) {
    if !rect.is_empty() && color.to_rgba()[3] != 0 {
        builder.fill(RoundedRect::new(rect, radius), color);
    }
}

fn border(builder: &mut SceneBuilder, rect: Rect, appearance: Appearance) {
    stroke(
        builder,
        rect,
        appearance.radius,
        appearance.border_width,
        appearance.border_color,
    );
}

/// Strokes inside `rect`, saturating at half its smaller side.
fn stroke(builder: &mut SceneBuilder, rect: Rect, radius: f32, width: f32, color: Color) {
    let width = width.min(rect.size.width.min(rect.size.height) * 0.5);
    if width > 0.0 && color.to_rgba()[3] != 0 {
        let inset = width * 0.5;
        builder.stroke(
            RoundedRect::new(
                Rect::new(
                    rect.origin.x + inset,
                    rect.origin.y + inset,
                    rect.size.width - width,
                    rect.size.height - width,
                ),
                (radius - inset).max(0.0),
            ),
            color,
            width,
        );
    }
}

/// Draws a polyline through `points` as round-capped bars of `thickness`,
/// without allocating a path.
fn polyline(builder: &mut SceneBuilder, points: &[(f32, f32)], thickness: f32, color: Color) {
    let half = thickness * 0.5;
    for pair in points.windows(2) {
        let [(x0, y0), (x1, y1)] = [pair[0], pair[1]];
        let length = (x1 - x0).hypot(y1 - y0);
        if length == 0.0 {
            continue;
        }
        let (cosine, sine) = ((x1 - x0) / length, (y1 - y0) / length);
        builder.push_transform(Affine::new([cosine, sine, -sine, cosine, x0, y0]).or_fail());
        fill(
            builder,
            Rect::new(-half, -half, length + thickness, thickness),
            half,
            color,
        );
        builder.pop();
    }
}

/// A check mark filling a `size`-pixel box whose corner is `(x, y)`.
pub fn check_mark(builder: &mut SceneBuilder, x: f32, y: f32, size: f32, color: Color) {
    let at = |u: f32, v: f32| (x + u * size, y + v * size);
    polyline(
        builder,
        &[at(0.27, 0.52), at(0.43, 0.68), at(0.74, 0.34)],
        (size * 0.09).max(1.5),
        color,
    );
}

/// Direction for a chevron arrow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChevronDirection {
    /// Pointing downwards (expanded).
    Down,
    /// Pointing to the right (collapsed in LTR).
    Right,
    /// Pointing to the left (collapsed right to left).
    Left,
}

/// A thin chevron [`CHEVRON`] wide pointing in `direction`, centered at `(x, y)`.
pub fn chevron_dir(
    builder: &mut SceneBuilder,
    x: f32,
    y: f32,
    direction: ChevronDirection,
    color: Color,
) {
    let (w, h) = (CHEVRON * 0.5, CHEVRON * 0.25);
    let points = match direction {
        ChevronDirection::Down => [(x - w, y - h), (x, y + h), (x + w, y - h)],
        ChevronDirection::Right => [(x - h, y - w), (x + h, y), (x - h, y + w)],
        ChevronDirection::Left => [(x + h, y - w), (x - h, y), (x + h, y + w)],
    };
    polyline(builder, &points, 1.5, color);
}

/// A downward chevron centered at `(x, y)`, for dropdowns.
pub fn chevron(builder: &mut SceneBuilder, x: f32, y: f32, color: Color) {
    chevron_dir(builder, x, y, ChevronDirection::Down, color)
}
