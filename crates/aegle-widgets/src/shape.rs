//! Filled and stroked outlines: a path, or an arc of the largest circle that
//! fits the bounds.

use aegle_scene::{Color, FillRule, LineCap, LineJoin, Path, PathBuilder, Point, Stroke};
use aegle_theme::ControlKind;
use aegle_ui::{Container, Control, LayoutStyle, control::PaintCx, handle};

handle! {
    /// A vector outline, filled and stroked, with no intrinsic size: give it
    /// one. Nothing draws until it has an outline and a visible fill or stroke.
    pub Shape(ShapeControl)
}

impl Shape {
    /// Draws `path` in local coordinates, or nothing with `None`.
    pub fn set_path(&self, path: Option<Path>) {
        self.update(|shape| shape.outline = path.map(Outline::Path))
    }
    /// Draws the arc from `start` for `sweep` degrees, clockwise from the
    /// top, on the largest circle that fits the bounds with its stroke.
    pub fn set_arc(&self, start: f32, sweep: f32) {
        self.update(|shape| shape.outline = Some(Outline::Arc { start, sweep }))
    }
    /// The arc's start and sweep in degrees, if it draws an arc.
    pub fn arc(&self) -> Option<(f32, f32)> {
        self.read(|shape| match shape.outline {
            Some(Outline::Arc { start, sweep }) => Some((start, sweep)),
            _ => None,
        })
    }
    /// The color filling the outline; transparent by default.
    pub fn set_fill(&self, color: Color) {
        self.update(|shape| shape.fill = color)
    }
    /// The color stroking the outline; transparent by default.
    pub fn set_stroke(&self, color: Color) {
        self.update(|shape| shape.stroke = color)
    }
    /// The stroke's nonnegative width; 1 by default.
    pub fn set_stroke_width(&self, width: f32) {
        assert!(width.is_finite() && width >= 0.0, "invalid stroke width");
        self.update(|shape| shape.stroke_style.width = width)
    }
    /// How open ends of the stroke look; butt by default.
    pub fn set_cap(&self, cap: LineCap) {
        self.update(|shape| shape.stroke_style.cap = cap)
    }
}

enum Outline {
    Path(Path),
    Arc { start: f32, sweep: f32 },
}

/// The control inside a [`Shape`] node.
pub struct ShapeControl {
    outline: Option<Outline>,
    fill: Color,
    stroke: Color,
    stroke_style: Stroke,
}

impl Control for ShapeControl {
    fn kind(&self) -> &'static ControlKind {
        &aegle_ui::CONTAINER
    }
    fn paint(&mut self, cx: &mut PaintCx<'_>) {
        let path = match &self.outline {
            None => return,
            Some(Outline::Path(path)) => path.clone(),
            Some(Outline::Arc { start, sweep }) => {
                let (start, sweep) = (*start, *sweep);
                let size = cx.size;
                let radius = (size.width.min(size.height) - self.stroke_style.width) / 2.0;
                if radius <= 0.0 || sweep == 0.0 {
                    return;
                }
                let center = Point::new(size.width / 2.0, size.height / 2.0);
                // Zero degrees points up, as for rotation.
                let start = (start - 90.0).to_radians();
                let mut builder = PathBuilder::new();
                builder.arc(
                    center,
                    radius,
                    start,
                    sweep.clamp(-360.0, 360.0).to_radians(),
                );
                builder.finish(FillRule::NonZero)
            }
        };
        if self.fill.to_rgba()[3] > 0 {
            cx.builder.fill_path(&path, self.fill);
        }
        if self.stroke.to_rgba()[3] > 0 && self.stroke_style.width > 0.0 {
            cx.builder
                .stroke_path(&path, self.stroke, self.stroke_style);
        }
    }
    #[cfg(feature = "accessibility")]
    fn semantics(&self, cx: &mut aegle_ui::control::SemanticsCx<'_>) {
        cx.set_role(aegle_ui::Role::Image);
    }
}

pub(crate) fn shape(container: &Container) -> Shape {
    Shape(crate::add(container, |_, _| {
        let mut stroke_style = Stroke::new(1.0);
        stroke_style.join = LineJoin::Round;
        Ok((
            Box::new(ShapeControl {
                outline: None,
                fill: Color::TRANSPARENT,
                stroke: Color::TRANSPARENT,
                stroke_style,
            }) as Box<dyn Control>,
            {
                let mut style = LayoutStyle::default();
                style.set_shrink(0.0);
                style
            },
        ))
    }))
}
