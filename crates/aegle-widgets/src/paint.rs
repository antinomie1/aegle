//! Check boxes, radio buttons, switches, sliders, progress bars and dropdown marks.
use aegle_scene::{Affine, Color, FillRule, PathBuilder, Point, Rect, RoundedRect, SceneBuilder};
use aegle_theme::Appearance;
use aegle_types::Size;
use aegle_ui::OrFail;

/// Width of the dropdown chevron, shared with layout.
pub const CHEVRON: f32 = 8.0;

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
    /// Size of the label, or `None` when there is no label to paint.
    pub label: Option<Size>,
    /// Laid out right to left: the marker sits at the right edge with the label
    /// to its left, and a switch's thumb moves leftward when on.
    pub rtl: bool,
}

/// Paints a toggle. `label` receives the builder, already translated to the
/// label origin, and the foreground color; it runs only when the spec has a label.
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
        label: label_size,
        rtl,
    } = *spec;
    builder.push_clip(RoundedRect::new(
        Rect::new(0.0, 0.0, size.width, size.height),
        0.0,
    ));
    let x = padding.min(size.width * 0.5);
    let y_padding = padding.min(size.height * 0.5);
    let switch = mark == Mark::Switch;
    let (width, height) = if switch { (36.0, 20.0) } else { (18.0, 18.0) };
    let scale = ((size.width - 2.0 * x) / width)
        .min((size.height - 2.0 * y_padding) / height)
        .min(1.0);
    let (width, height) = (width * scale, height * scale);
    let y = (size.height - height) * 0.5;
    let x = if rtl { size.width - x - width } else { x };
    let marker = Rect::new(x, y, width, height);
    // Radio buttons are round, so their shape differs from check boxes.
    let frame = match mark {
        Mark::Radio => Appearance {
            radius: width * 0.5,
            ..appearance
        },
        _ => appearance,
    };
    border(builder, marker, frame);
    if mark == Mark::Radio {
        if checked {
            let diameter = width * 0.44;
            let inset = (width - diameter) * 0.5;
            let dot = Rect::new(x + inset, y + inset, diameter, diameter);
            fill(builder, dot, diameter * 0.5, appearance.indicator);
        }
    } else if mixed {
        let thickness = width * 0.115;
        let bar = Rect::new(
            x + width * 0.25,
            y + (height - thickness) * 0.5,
            width * 0.5,
            thickness,
        );
        fill(builder, bar, 0.0, appearance.indicator);
    } else if switch {
        let inset = (appearance.border_width + 2.0 * scale).min(height * 0.5);
        let diameter = (height - 2.0 * inset).max(0.0);
        let thumb_x = if checked != rtl {
            x + width - inset - diameter
        } else {
            x + inset
        };
        fill(
            builder,
            Rect::new(thumb_x, y + inset, diameter, diameter),
            appearance.radius,
            if checked {
                appearance.indicator
            } else {
                appearance.border_color
            },
        );
    } else if checked {
        check_mark(builder, x, y, width, appearance.indicator);
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
    let diameter = 16.0_f32.min(size.width).min(size.height);
    let start = (padding + diameter * 0.5).min(size.width * 0.5);
    (start, (size.width - 2.0 * start).max(0.0))
}

/// Paints a horizontal slider (with its thumb at `filled[1]`) or progress bar
/// whose indicator covers the `filled` span of fractions, `0.0..=1.0`.
pub fn range(
    builder: &mut SceneBuilder,
    size: Size,
    padding: f32,
    filled: [f64; 2],
    slider: bool,
    appearance: Appearance,
) {
    builder.push_clip(RoundedRect::new(
        Rect::new(0.0, 0.0, size.width, size.height),
        0.0,
    ));
    let (start, length) = if slider {
        slider_track(size, padding)
    } else {
        let start = padding.min(size.width * 0.5);
        (start, (size.width - 2.0 * start).max(0.0))
    };
    let height = 4.0_f32.min(size.height);
    let y = (size.height - height) * 0.5;
    // A thinner resting track keeps progress legible without relying on color.
    let track = Rect::new(start, y + height * 0.25, length, height * 0.5);
    fill(builder, track, appearance.radius, appearance.background);
    let [from, to] = filled.map(|f| length * f.clamp(0.0, 1.0) as f32);
    fill(
        builder,
        Rect::new(start + from, y, (to - from).max(0.0), height),
        appearance.radius,
        appearance.indicator,
    );
    border(builder, track, appearance);
    if slider {
        let diameter = 16.0_f32.min(size.width).min(size.height);
        let thumb = Rect::new(
            start + to - diameter * 0.5,
            (size.height - diameter) * 0.5,
            diameter,
            diameter,
        );
        fill(builder, thumb, appearance.radius, appearance.indicator);
        border(builder, thumb, appearance);
    }
    builder.pop();
}

fn fill(builder: &mut SceneBuilder, rect: Rect, radius: f32, color: Color) {
    if !rect.is_empty() && color.to_rgba()[3] != 0 {
        builder.fill(RoundedRect::new(rect, radius), color);
    }
}

fn border(builder: &mut SceneBuilder, rect: Rect, appearance: Appearance) {
    let width = appearance
        .border_width
        .min(rect.size.width.min(rect.size.height) * 0.5);
    if width > 0.0 && appearance.border_color.to_rgba()[3] != 0 {
        let inset = width * 0.5;
        builder.stroke(
            RoundedRect::new(
                Rect::new(
                    rect.origin.x + inset,
                    rect.origin.y + inset,
                    rect.size.width - width,
                    rect.size.height - width,
                ),
                (appearance.radius - inset).max(0.0),
            ),
            appearance.border_color,
            width,
        );
    }
}

/// A check mark filling a `size`-pixel box whose corner is `(x, y)`.
pub fn check_mark(builder: &mut SceneBuilder, x: f32, y: f32, size: f32, color: Color) {
    let half_thickness = size * 0.0575;
    let cosine = std::f32::consts::FRAC_1_SQRT_2;
    for (start_x, start_y, length, sine) in
        [(0.22, 0.49, 0.21, cosine), (0.43, 0.70, 0.37, -cosine)]
    {
        builder.push_transform(
            Affine::new([
                cosine,
                sine,
                -sine,
                cosine,
                x + start_x * size,
                y + start_y * size,
            ])
            .or_fail(),
        );
        fill(
            builder,
            Rect::new(
                0.0,
                -half_thickness,
                length * size * std::f32::consts::SQRT_2,
                2.0 * half_thickness,
            ),
            half_thickness,
            color,
        );
        builder.pop();
    }
}

/// A downward chevron centered at `(x, y)`, for dropdowns.
pub fn chevron(builder: &mut SceneBuilder, x: f32, y: f32, color: Color) {
    let mut path = PathBuilder::new();
    path.move_to(Point::new(x - CHEVRON * 0.5, y - CHEVRON * 0.25));
    path.line_to(Point::new(x + CHEVRON * 0.5, y - CHEVRON * 0.25));
    path.line_to(Point::new(x, y + CHEVRON * 0.35));
    path.close();
    builder.fill_path(&path.finish(FillRule::NonZero), color);
}
